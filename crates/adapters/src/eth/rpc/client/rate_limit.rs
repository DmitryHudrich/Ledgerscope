use std::{
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use alloy::{
    rpc::json_rpc::{ErrorPayload, RequestPacket, ResponsePacket},
    transports::{RpcError, TransportError, TransportErrorKind, TransportFut},
};
use tokio::{
    sync::Mutex,
    time::{Instant, sleep_until},
};
use tower::{Layer, Service};

const MAX_ATTEMPTS: u32 = 5;
const BASE_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(8);

pub struct RateLimitLayer {
    limiter: Arc<RateLimiter>,
}

impl RateLimitLayer {
    pub fn new(rps: u32) -> Self {
        Self {
            limiter: Arc::new(RateLimiter::new(rps)),
        }
    }
}

impl<S> Layer<S> for RateLimitLayer {
    type Service = RateLimited<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RateLimited {
            inner,
            limiter: self.limiter.clone(),
        }
    }
}

#[derive(Clone)]
pub struct RateLimited<S> {
    inner: S,
    limiter: Arc<RateLimiter>,
}

impl<S> Service<RequestPacket> for RateLimited<S>
where
    S: Service<
            RequestPacket,
            Response = ResponsePacket,
            Error = TransportError,
            Future = TransportFut<'static>,
        > + Clone
        + Send
        + Sync
        + 'static,
{
    type Response = ResponsePacket;
    type Error = TransportError;
    type Future = TransportFut<'static>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: RequestPacket) -> Self::Future {
        let limiter = self.limiter.clone();
        let spare = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, spare);

        let requests = request.len();
        let method = request
            .method_names()
            .next()
            .unwrap_or("rpc request")
            .to_owned();

        Box::pin(async move {
            let mut attempt = 1;

            loop {
                limiter.acquire(requests).await;

                let retry = match inner.call(request.clone()).await {
                    Ok(response) => match rate_limited_response(&response) {
                        Some(retry) => retry,
                        None => return Ok(response),
                    },
                    Err(error) => match retryable(error) {
                        Ok(retry) => retry,
                        Err(error) => return Err(error),
                    },
                };

                if attempt >= MAX_ATTEMPTS {
                    return Err(TransportErrorKind::custom_str(&format!(
                        "{method} gave up after {attempt} attempts: {}",
                        retry.reason
                    )));
                }

                let delay = retry.after.unwrap_or_else(|| backoff(attempt));
                tracing::warn!(
                    "{method} attempt {attempt}/{MAX_ATTEMPTS} failed ({}), pausing every caller for {delay:?}",
                    retry.reason
                );
                limiter.throttle(delay).await;
                attempt += 1;
            }
        })
    }
}

struct Retry {
    reason: String,
    after: Option<Duration>,
}

fn rate_limited_response(response: &ResponsePacket) -> Option<Retry> {
    let error = response.iter_errors().find(|error| rate_limited(error))?;

    Some(Retry {
        reason: error.to_string(),
        after: None,
    })
}

fn retryable(error: TransportError) -> Result<Retry, TransportError> {
    let retry = match &error {
        RpcError::Transport(kind) => retryable_transport(kind),
        RpcError::ErrorResp(payload) => rate_limited(payload).then(|| Retry {
            reason: payload.to_string(),
            after: None,
        }),
        _ => None,
    };

    retry.ok_or(error)
}

fn retryable_transport(kind: &TransportErrorKind) -> Option<Retry> {
    let server_error = kind
        .as_http_error()
        .is_some_and(|error| error.status >= 500);
    let connection_error = kind.as_custom().is_some();

    (kind.is_retry_err() || server_error || connection_error).then(|| Retry {
        reason: kind.to_string(),
        after: kind.retry_after().map(|after| after.min(MAX_BACKOFF)),
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
        || message.contains("too many requests")
}

fn backoff(attempt: u32) -> Duration {
    let exponential = BASE_BACKOFF * 2u32.saturating_pow(attempt - 1);
    let jitter = Duration::from_millis(u64::from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.subsec_millis() % 100)
            .unwrap_or(0),
    ));

    exponential.min(MAX_BACKOFF) + jitter
}

struct RateLimiter {
    interval: Duration,
    next_slot: Mutex<Instant>,
}

impl RateLimiter {
    fn new(rps: u32) -> Self {
        Self {
            interval: Duration::from_secs(1) / rps.max(1),
            next_slot: Mutex::new(Instant::now()),
        }
    }

    async fn acquire(&self, requests: usize) {
        let requests = u32::try_from(requests.max(1)).unwrap_or(u32::MAX);

        let slot = {
            let mut next_slot = self.next_slot.lock().await;
            let slot = (*next_slot).max(Instant::now());
            *next_slot = slot + self.interval * requests;
            slot
        };

        sleep_until(slot).await;
    }

    async fn throttle(&self, cooldown: Duration) {
        let resume = Instant::now() + cooldown;
        let mut next_slot = self.next_slot.lock().await;
        *next_slot = (*next_slot).max(resume);
    }
}
