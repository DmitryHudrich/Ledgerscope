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
const METHOD_NOT_FOUND: i64 = -32601;
const BLOCK_LOOKUPS: [&str; 2] = ["eth_getBlockByNumber", "eth_getBlockReceipts"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryKind {
    Busy,
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
            RetryKind::Busy
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

    if status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::FORBIDDEN
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
        let kind = if lowered.contains("frequency")
            || lowered.contains("limit")
            || lowered.contains("busy")
        {
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
    let server_error = kind
        .as_http_error()
        .is_some_and(|error| error.status >= 500 || error.status == 429 || error.status == 403);
    let connection_error = kind.as_custom().is_some();

    (kind.is_retry_err() || server_error || connection_error).then(|| Retry {
        reason: kind.to_string(),
        after: kind.retry_after().map(|after| after.min(MAX_BACKOFF)),
        kind: RetryKind::Busy,
    })
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
            Some(RetryKind::Busy)
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

    #[test]
    fn a_revert_is_the_answer_not_a_node_problem() {
        assert_eq!(kind(3, "execution reverted"), None);
        assert_eq!(kind(-32000, "insufficient funds for gas"), None);
    }
}
