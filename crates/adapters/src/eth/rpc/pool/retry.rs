use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::StatusCode;
use serde_json::Value;

use super::Failure;

use alloy::{
    rpc::json_rpc::{ErrorPayload, ResponsePacket, ResponsePayload},
    transports::{RpcError, TransportError, TransportErrorKind},
};

const BASE_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(8);
const MAX_HINT: Duration = Duration::from_secs(30);
const CLIENT_CLOSED: u16 = 499;
const METHOD_NOT_FOUND: i64 = -32601;
const BLOCK_LOOKUPS: [&str; 2] = ["eth_getBlockByNumber", "eth_getBlockReceipts"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryKind {
    Busy,
    Limited,
    Unsupported,
    Shallow,
}

pub struct Retry {
    pub reason: String,
    pub after: Option<Duration>,
    pub kind: RetryKind,
}

impl Retry {
    fn busy(reason: String) -> Self {
        Self {
            reason,
            after: None,
            kind: RetryKind::Busy,
        }
    }

    fn of(error: &ErrorPayload) -> Option<Self> {
        let kind = if unsupported(error) {
            RetryKind::Unsupported
        } else if shallow(error) {
            RetryKind::Shallow
        } else if rate_limited(error) {
            RetryKind::Limited
        } else {
            return None;
        };

        Some(Self {
            reason: error.to_string(),
            after: None,
            kind,
        })
    }
}

pub fn failed_response(response: &ResponsePacket, requests: usize, method: &str) -> Option<Retry> {
    let answers = match response {
        ResponsePacket::Single(answer) => std::slice::from_ref(answer),
        ResponsePacket::Batch(answers) => answers.as_slice(),
    };
    if answers.len() < requests {
        return Some(Retry::busy(format!(
            "answered {} of {requests} batched calls",
            answers.len()
        )));
    }

    let missing = answers
        .iter()
        .filter(|answer| matches!(&answer.payload, ResponsePayload::Success(raw) if raw.get().trim() == "null"))
        .count();
    if missing > 0 && BLOCK_LOOKUPS.contains(&method) {
        return Some(Retry::busy(format!(
            "knows nothing about {missing} of {} blocks",
            answers.len()
        )));
    }

    response.iter_errors().find_map(Retry::of)
}

pub fn retryable(error: TransportError) -> Result<Retry, TransportError> {
    let retry = match &error {
        RpcError::Transport(kind) => retryable_transport(kind),
        RpcError::ErrorResp(payload) => Retry::of(payload),
        RpcError::DeserError { err, .. } => Some(Retry::busy(format!("unreadable answer: {err}"))),
        _ => None,
    };

    retry.ok_or(error)
}

pub fn http_unreachable(error: &reqwest::Error) -> Retry {
    Retry::busy(format!("unreachable: {error}"))
}

pub fn http_answer(
    status: StatusCode,
    text: &str,
    useful: fn(&Value) -> bool,
) -> Result<Value, Failure<std::io::Error>> {
    let excerpt: String = text.trim().chars().take(200).collect();

    if status == StatusCode::TOO_MANY_REQUESTS {
        return Err(Failure::Retry(Retry {
            reason: format!("{status}: {excerpt}"),
            after: None,
            kind: RetryKind::Limited,
        }));
    }
    if status == StatusCode::FORBIDDEN
        || status.as_u16() == CLIENT_CLOSED
        || status.is_server_error()
    {
        return Err(Failure::Retry(Retry::busy(format!("{status}: {excerpt}"))));
    }
    if !status.is_success() {
        return Err(Failure::Retry(Retry {
            reason: format!("{status}: {excerpt}"),
            after: None,
            kind: RetryKind::Unsupported,
        }));
    }

    let Ok(answer) = serde_json::from_str::<Value>(text) else {
        return Err(Failure::Retry(Retry::busy(format!(
            "unreadable answer: {excerpt}"
        ))));
    };

    if let Some(message) = refusal(&answer) {
        let lowered = message.to_lowercase();
        let kind = if lowered.contains("frequency") || lowered.contains("limit") {
            RetryKind::Limited
        } else if lowered.contains("busy") {
            RetryKind::Busy
        } else if lowered.contains("not available")
            || lowered.contains("plan")
            || lowered.contains("not supported")
        {
            RetryKind::Unsupported
        } else {
            return Err(Failure::Fatal(std::io::Error::other(message)));
        };
        return Err(Failure::Retry(Retry {
            reason: message,
            after: None,
            kind,
        }));
    }

    if !useful(&answer) {
        return Err(Failure::Retry(Retry::busy(format!(
            "answered nothing useful: {excerpt}"
        ))));
    }

    Ok(answer)
}

fn refusal(answer: &Value) -> Option<String> {
    if let Some(error) = answer.get("Error").and_then(Value::as_str) {
        return Some(error.to_owned());
    }
    let code = answer.get("code")?;
    if code.is_string() {
        return None;
    }
    answer
        .get("message")
        .and_then(Value::as_str)
        .map(|message| format!("{code}: {message}"))
}

fn retryable_transport(kind: &TransportErrorKind) -> Option<Retry> {
    let Some(error) = kind.as_http_error() else {
        return (kind.is_retry_err() || kind.as_custom().is_some())
            .then(|| Retry::busy(kind.to_string()));
    };

    let refusal = refusal_in(&error.body);
    let refused = refusal
        .as_ref()
        .and_then(payload_of)
        .and_then(|payload| Retry::of(&payload))
        .map(|retry| retry.kind);
    let status = error.status;
    let retry_kind = match refused {
        Some(RetryKind::Unsupported) => RetryKind::Unsupported,
        Some(RetryKind::Shallow) => RetryKind::Shallow,
        _ if status == 429 => RetryKind::Limited,
        Some(refused) => refused,
        None if status >= 500 || status == 403 || status == CLIENT_CLOSED => RetryKind::Busy,
        None if kind.is_retry_err() => RetryKind::Busy,
        None => return None,
    };

    let after = kind
        .retry_after()
        .or_else(|| refusal.as_ref().and_then(hint_of))
        .map(|after| after.min(MAX_HINT));

    Some(Retry {
        reason: kind.to_string(),
        after,
        kind: retry_kind,
    })
}

fn refusal_in(body: &str) -> Option<Value> {
    let mut answer: Value = serde_json::from_str(body).ok()?;
    answer.get_mut("error").map(Value::take)
}

fn payload_of(error: &Value) -> Option<ErrorPayload> {
    Some(ErrorPayload {
        code: error.get("code")?.as_i64()?,
        message: error.get("message")?.as_str()?.to_owned().into(),
        data: None,
    })
}

fn hint_of(error: &Value) -> Option<Duration> {
    error
        .pointer("/data/retry_after_ms")
        .and_then(Value::as_u64)
        .map(Duration::from_millis)
}

pub fn retry_after(header: &str) -> Option<Duration> {
    header
        .trim()
        .parse::<u64>()
        .ok()
        .map(|seconds| Duration::from_secs(seconds).min(MAX_HINT))
}

fn rate_limited(error: &ErrorPayload) -> bool {
    if error.is_retry_err() {
        return true;
    }

    let message = error.message.to_lowercase();

    message.contains("rate limit")
        || message.contains("request limit")
        || message.contains("limit reached")
        || message.contains("limited to")
        || message.contains("too many requests")
        || message.contains("busy")
}

fn unsupported(error: &ErrorPayload) -> bool {
    if error.code == METHOD_NOT_FOUND {
        return true;
    }

    let message = error.message.to_lowercase();

    message.contains("method not found")
        || message.contains("not whitelisted")
        || message.contains("not supported")
        || message.contains("unsupported method")
        || message.contains("not available")
}

fn shallow(error: &ErrorPayload) -> bool {
    let message = error.message.to_lowercase();

    message.contains("history")
        || message.contains("pruned")
        || message.contains("missing trie node")
        || message.contains("header not found")
        || message.contains("unknown block")
        || message.contains("block not found")
        || message.contains("archive")
        || message.contains("state window")
        || message.contains("state is not available")
}

pub fn backoff(attempt: u32) -> Duration {
    let exponential = BASE_BACKOFF * 2u32.saturating_pow(attempt - 1);
    let jitter = Duration::from_millis(u64::from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.subsec_millis() % 100)
            .unwrap_or(0),
    ));

    exponential.min(MAX_BACKOFF) + jitter
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(code: i64, message: &str) -> ErrorPayload {
        ErrorPayload {
            code,
            message: message.to_owned().into(),
            data: None,
        }
    }

    fn kind(code: i64, message: &str) -> Option<RetryKind> {
        Retry::of(&payload(code, message)).map(|retry| retry.kind)
    }

    #[test]
    fn tells_apart_why_a_node_refused() {
        assert_eq!(
            kind(
                -32011,
                "block reference is outside the public history window"
            ),
            Some(RetryKind::Shallow)
        );
        assert_eq!(
            kind(-32000, "missing trie node abc"),
            Some(RetryKind::Shallow)
        );
        assert_eq!(
            kind(-32005, "public chain pool is busy"),
            Some(RetryKind::Limited)
        );
        assert_eq!(
            kind(-32601, "the method does not exist"),
            Some(RetryKind::Unsupported)
        );
        assert_eq!(
            kind(
                -32012,
                "Method \"eth_call\" is not available for unregistered accounts"
            ),
            Some(RetryKind::Unsupported)
        );
    }

    fn transport(status: u16, body: &str, after: Option<Duration>) -> Option<Retry> {
        let error = TransportErrorKind::http_error_with_retry_after(status, body.to_owned(), after);
        match error {
            RpcError::Transport(kind) => retryable_transport(&kind),
            _ => None,
        }
    }

    #[test]
    fn a_rate_limit_answer_slows_the_node_down_for_as_long_as_it_asks() {
        let retry = transport(
            429,
            r#"{"error":{"code":-32029,"data":{"retry_after_ms":25000},"message":"rate limit exceeded"},"id":1,"jsonrpc":"2.0"}"#,
            None,
        )
        .unwrap();
        assert_eq!(retry.kind, RetryKind::Limited);
        assert_eq!(retry.after, Some(Duration::from_secs(25)));

        let retry = transport(429, "error code: 1015", Some(Duration::from_secs(600))).unwrap();
        assert_eq!(retry.kind, RetryKind::Limited);
        assert_eq!(retry.after, Some(MAX_HINT));
    }

    #[test]
    fn a_forbidden_method_is_unsupported_not_busy() {
        let retry = transport(
            403,
            r#"{"jsonrpc":"2.0","error":{"code":-32601,"message":"rpc method is not whitelisted"},"id":7028}"#,
            None,
        )
        .unwrap();
        assert_eq!(retry.kind, RetryKind::Unsupported);
    }

    #[test]
    fn a_dropped_request_is_retried() {
        assert_eq!(transport(499, "", None).unwrap().kind, RetryKind::Busy);
        assert!(transport(400, "bad request", None).is_none());
    }

    #[test]
    fn a_revert_is_the_answer_not_a_node_problem() {
        assert_eq!(kind(3, "execution reverted"), None);
        assert_eq!(kind(-32000, "insufficient funds for gas"), None);
    }
}
