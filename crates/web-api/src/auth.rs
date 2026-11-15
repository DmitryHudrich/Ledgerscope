use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::FromRequestParts,
    http::{
        HeaderMap, HeaderValue,
        header::{COOKIE, SET_COOKIE},
        request::Parts,
    },
};

use application::account::{AccountError, User};

use crate::{
    routes::ApiError,
    state::{Accounts, AppState},
};

pub const SESSION_COOKIE: &str = "ledgerscope_session";

const THROTTLE_WINDOW: Duration = Duration::from_secs(15 * 60);
const THROTTLE_ATTEMPTS: u32 = 10;
const THROTTLE_SWEEP_AT: usize = 10_000;

pub fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

pub fn session_cookie(token: &str, max_age_secs: i64, secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}{secure}"
    ))
    .expect("a session token is url-safe base64")
}

pub fn cleared_cookie(secure: bool) -> HeaderValue {
    session_cookie("", 0, secure)
}

pub fn set_cookie(headers: &mut HeaderMap, cookie: HeaderValue) {
    headers.append(SET_COOKIE, cookie);
}

pub fn accounts(state: &AppState) -> Result<&Arc<Accounts>, ApiError> {
    state
        .accounts()
        .ok_or_else(|| ApiError::unavailable("accounts are off on this server"))
}

pub struct CurrentUser {
    pub user: User,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let accounts = accounts(state)?;
        let token = session_token(&parts.headers).ok_or(AccountError::Unauthenticated)?;
        let user = accounts
            .auth
            .authenticate(&token)
            .await?
            .ok_or(AccountError::Unauthenticated)?;
        Ok(Self { user })
    }
}

pub struct MaybeUser(pub Option<User>);

impl FromRequestParts<AppState> for MaybeUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let (Some(accounts), Some(token)) = (state.accounts(), session_token(&parts.headers))
        else {
            return Ok(Self(None));
        };
        match accounts.auth.authenticate(&token).await {
            Ok(user) => Ok(Self(user)),
            Err(error) => {
                tracing::warn!("could not check a session, serving it anonymously: {error}");
                Ok(Self(None))
            }
        }
    }
}

#[derive(Default)]
pub struct LoginThrottle {
    failures: Mutex<HashMap<String, (u32, Instant)>>,
}

impl LoginThrottle {
    pub fn check(&self, key: &str) -> Result<(), ApiError> {
        let failures = self.failures.lock().expect("throttle lock poisoned");
        match failures.get(key) {
            Some((count, since))
                if *count >= THROTTLE_ATTEMPTS && since.elapsed() < THROTTLE_WINDOW =>
            {
                let wait = THROTTLE_WINDOW.saturating_sub(since.elapsed()).as_secs() / 60 + 1;
                Err(ApiError::too_many(format!(
                    "too many failed sign-ins, try again in {wait} min"
                )))
            }
            _ => Ok(()),
        }
    }

    pub fn failed(&self, key: &str) {
        let mut failures = self.failures.lock().expect("throttle lock poisoned");
        if failures.len() >= THROTTLE_SWEEP_AT {
            failures.retain(|_, (_, since)| since.elapsed() < THROTTLE_WINDOW);
        }
        let entry = failures
            .entry(key.to_owned())
            .or_insert((0, Instant::now()));
        if entry.1.elapsed() >= THROTTLE_WINDOW {
            *entry = (0, Instant::now());
        }
        entry.0 += 1;
    }

    pub fn succeeded(&self, key: &str) {
        self.failures
            .lock()
            .expect("throttle lock poisoned")
            .remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_cookie_is_found_among_others() {
        let mut headers = HeaderMap::new();
        headers.append(
            COOKIE,
            HeaderValue::from_static("theme=dark; ledgerscope_session=abc_-1 ; x=y"),
        );

        assert_eq!(session_token(&headers).as_deref(), Some("abc_-1"));
    }

    #[test]
    fn an_empty_session_cookie_counts_as_none() {
        let mut headers = HeaderMap::new();
        headers.append(COOKIE, HeaderValue::from_static("ledgerscope_session="));

        assert_eq!(session_token(&headers), None);
    }

    #[test]
    fn the_cookie_is_http_only_and_lax() {
        let cookie = session_cookie("tok", 60, true);
        let cookie = cookie.to_str().unwrap();

        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.ends_with("Secure"));
        assert!(
            !session_cookie("tok", 60, false)
                .to_str()
                .unwrap()
                .contains("Secure")
        );
    }

    #[test]
    fn the_throttle_closes_after_ten_failures_and_reopens_on_success() {
        let throttle = LoginThrottle::default();
        for _ in 0..THROTTLE_ATTEMPTS {
            assert!(throttle.check("a@b.co").is_ok());
            throttle.failed("a@b.co");
        }

        assert!(throttle.check("a@b.co").is_err());
        assert!(throttle.check("c@d.co").is_ok());
        throttle.succeeded("a@b.co");
        assert!(throttle.check("a@b.co").is_ok());
    }
}
