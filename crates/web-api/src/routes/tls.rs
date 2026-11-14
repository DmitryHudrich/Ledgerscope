use std::{io, net::IpAddr, str::FromStr};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    routing::{get, post},
};
use serde::Deserialize;
use utoipa::IntoParams;

use application::observe::TlsCorrelationService;
use domain::observe::{ObservedIntent, TlsObservation};

use crate::{
    auth::{CurrentUser, accounts},
    dto::tls::{CorrelateResponse, IpWalletLinkResponse, ObservationRequest},
    routes::{ApiError, ErrorResponse, chain},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tls/observations", post(ingest))
        .route("/tls/links", get(links).delete(reset))
        .route("/tls/correlate", post(correlate))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ChainQuery {
    chain: Option<String>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CorrelateQuery {
    chain: Option<String>,
    from_block: Option<u64>,
    up_to_block: Option<u64>,
    reset: Option<bool>,
}

fn storage_failed(error: io::Error) -> ApiError {
    tracing::error!("tls observation storage failed: {error}");
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "storage failed, try again")
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let token = raw.strip_prefix("Bearer ")?.trim();
    (!token.is_empty()).then(|| token.to_owned())
}

fn token_matches(provided: &str, expected: &str) -> bool {
    if provided.len() != expected.len() {
        return false;
    }
    provided
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[utoipa::path(
    post,
    path = "/tls/observations",
    tag = "tls",
    request_body = ObservationRequest,
    security(("observer_token" = [])),
    responses(
        (status = ACCEPTED, description = "The observation is stored"),
        (status = UNAUTHORIZED, description = "The agent token is missing or wrong", body = ErrorResponse),
        (status = BAD_REQUEST, description = "The observation is malformed", body = ErrorResponse),
        (status = SERVICE_UNAVAILABLE, description = "The tls observer is off", body = ErrorResponse),
    ),
)]
pub(crate) async fn ingest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ObservationRequest>,
) -> Result<StatusCode, ApiError> {
    let accounts = accounts(&state)?;
    let expected = accounts
        .observer_token
        .as_deref()
        .ok_or_else(|| ApiError::unavailable("tls ingest is closed on this server"))?;
    let provided = bearer(&headers).ok_or_else(|| ApiError::unauthorized("missing agent token"))?;
    if !token_matches(&provided, expected) {
        return Err(ApiError::unauthorized("the agent token is wrong"));
    }
    let store = accounts
        .observations
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("tls observer is off on this server"))?;

    let src_ip = IpAddr::from_str(request.src_ip.trim())
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "src_ip is not an ip address"))?;
    let intent = ObservedIntent::parse(request.intent.trim()).ok_or_else(|| {
        ApiError::new(StatusCode::BAD_REQUEST, "intent must be broadcast or query")
    })?;
    let confidence = request.confidence.clamp(0.0, 1.0);

    let observation =
        TlsObservation::new(src_ip, request.chain_id, intent, confidence, request.observed_at);
    store.insert(&observation).await.map_err(storage_failed)?;
    Ok(StatusCode::ACCEPTED)
}

#[utoipa::path(
    get,
    path = "/tls/links",
    tag = "tls",
    params(ChainQuery),
    responses(
        (status = OK, description = "IP to wallet links ranked by confidence", body = Vec<IpWalletLinkResponse>),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorResponse),
        (status = SERVICE_UNAVAILABLE, description = "The tls observer is off", body = ErrorResponse),
    ),
)]
pub(crate) async fn links(
    State(state): State<AppState>,
    _current: CurrentUser,
    Query(query): Query<ChainQuery>,
) -> Result<Json<Vec<IpWalletLinkResponse>>, ApiError> {
    let accounts = accounts(&state)?;
    let store = accounts
        .observations
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("tls observer is off on this server"))?;
    let chain = chain(&state, query.chain.as_deref())?;
    let found = store
        .links(chain.meta().chain_id)
        .await
        .map_err(storage_failed)?;
    Ok(Json(found.iter().map(IpWalletLinkResponse::from).collect()))
}

#[utoipa::path(
    post,
    path = "/tls/correlate",
    tag = "tls",
    params(CorrelateQuery),
    responses(
        (status = OK, description = "The correlation pass over new blocks", body = CorrelateResponse),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorResponse),
        (status = NOT_FOUND, description = "The chain is not served here", body = ErrorResponse),
        (status = SERVICE_UNAVAILABLE, description = "The tls observer is off", body = ErrorResponse),
    ),
)]
pub(crate) async fn correlate(
    State(state): State<AppState>,
    _current: CurrentUser,
    Query(query): Query<CorrelateQuery>,
) -> Result<Json<CorrelateResponse>, ApiError> {
    let accounts = accounts(&state)?;
    let store = accounts
        .observations
        .clone()
        .ok_or_else(|| ApiError::unavailable("tls observer is off on this server"))?;
    let chain = chain(&state, query.chain.as_deref())?;
    let chain_id = chain.meta().chain_id;

    if query.reset.unwrap_or(false) {
        store.reset(chain_id).await.map_err(storage_failed)?;
    }

    let up_to_block = match query.up_to_block {
        Some(block) => block,
        None => chain
            .head_block()
            .await
            .ok_or_else(|| ApiError::unavailable("the node did not tell its head block"))?,
    };

    let repository = chain.index_repo().cloned().ok_or_else(|| {
        ApiError::unavailable(
            "local tx index is off for this chain; correlation reads only indexed blocks, \
             turn on storage.persist_txs and scan the range first",
        )
    })?;
    let service = TlsCorrelationService::new(store, repository, accounts.correlation);
    let run = service
        .correlate(chain_id, query.from_block, up_to_block)
        .await
        .map_err(|error| ApiError::upstream(format!("correlation could not read the index: {error}")))?;
    Ok(Json(run.into()))
}

#[utoipa::path(
    delete,
    path = "/tls/links",
    tag = "tls",
    params(ChainQuery),
    responses(
        (status = NO_CONTENT, description = "Links and the watermark are cleared"),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorResponse),
        (status = SERVICE_UNAVAILABLE, description = "The tls observer is off", body = ErrorResponse),
    ),
)]
pub(crate) async fn reset(
    State(state): State<AppState>,
    _current: CurrentUser,
    Query(query): Query<ChainQuery>,
) -> Result<StatusCode, ApiError> {
    let accounts = accounts(&state)?;
    let store = accounts
        .observations
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("tls observer is off on this server"))?;
    let chain = chain(&state, query.chain.as_deref())?;
    store.reset(chain.meta().chain_id).await.map_err(storage_failed)?;
    Ok(StatusCode::NO_CONTENT)
}
