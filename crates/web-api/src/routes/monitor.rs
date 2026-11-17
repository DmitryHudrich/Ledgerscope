use std::{collections::HashMap, convert::Infallible, time::Duration};

use axum::{
    Json, Router,
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
};
use futures::Stream;
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;
use utoipa::IntoParams;

use crate::{
    dto::monitor::{BuildResponse, CallResponse, NodeHealthResponse, ScanResponse},
    routes::{ApiError, ErrorResponse, chain},
    state::AppState,
};

const DEFAULT_CALLS: usize = 300;
const MAX_CALLS: usize = 2_000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/monitor/calls", get(calls))
        .route("/monitor/nodes", get(nodes))
        .route("/monitor/scans", get(scans))
        .route("/monitor/stream", get(stream))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MonitorQuery {
    chain: Option<String>,
    limit: Option<usize>,
}

fn chain_filter(state: &AppState, selector: Option<&str>) -> Result<Option<u64>, ApiError> {
    match selector
        .map(str::trim)
        .filter(|selector| !selector.is_empty() && *selector != "all")
    {
        None => Ok(None),
        Some(selector) => Ok(Some(chain(state, Some(selector))?.meta().chain_id)),
    }
}

#[utoipa::path(
    get,
    path = "/monitor/calls",
    tag = "monitor",
    params(MonitorQuery),
    responses(
        (status = OK, description = "The latest rpc calls, oldest first", body = Vec<CallResponse>),
        (status = NOT_FOUND, description = "The chain is not served here", body = ErrorResponse),
    ),
)]
pub(crate) async fn calls(
    State(state): State<AppState>,
    Query(query): Query<MonitorQuery>,
) -> Result<Json<Vec<CallResponse>>, ApiError> {
    let chain_id = chain_filter(&state, query.chain.as_deref())?;
    let limit = query.limit.unwrap_or(DEFAULT_CALLS).clamp(1, MAX_CALLS);
    Ok(Json(
        state
            .monitor()
            .recent(chain_id, limit)
            .iter()
            .map(CallResponse::from)
            .collect(),
    ))
}

#[utoipa::path(
    get,
    path = "/monitor/nodes",
    tag = "monitor",
    params(MonitorQuery),
    responses((status = OK, description = "Every rpc node in the pools or heard from, with its health", body = Vec<NodeHealthResponse>)),
)]
pub(crate) async fn nodes(
    State(state): State<AppState>,
    Query(query): Query<MonitorQuery>,
) -> Result<Json<Vec<NodeHealthResponse>>, ApiError> {
    let chain_id = chain_filter(&state, query.chain.as_deref())?;
    let mut stats: HashMap<(u64, String), _> = state
        .monitor()
        .node_stats(chain_id)
        .into_iter()
        .map(|stats| ((stats.chain_id, stats.node.clone()), stats))
        .collect();

    let mut health = Vec::new();
    for served in state
        .chains()
        .filter(|served| chain_id.is_none_or(|chain_id| served.meta().chain_id == chain_id))
    {
        let id = served.meta().chain_id;
        for node in served.pool().nodes() {
            let seen = stats.remove(&(id, node.host.clone()));
            health.push(NodeHealthResponse::new(id, Some(&node), seen.as_ref()));
        }
    }
    let mut dropped: Vec<_> = stats.into_values().collect();
    dropped.sort_by(|a, b| a.chain_id.cmp(&b.chain_id).then(b.calls.cmp(&a.calls)));
    health.extend(
        dropped
            .iter()
            .map(|stats| NodeHealthResponse::new(stats.chain_id, None, Some(stats))),
    );

    Ok(Json(health))
}

#[utoipa::path(
    get,
    path = "/monitor/scans",
    tag = "monitor",
    params(MonitorQuery),
    responses((status = OK, description = "Block scans over rpc, running ones and the latest finished, newest first", body = Vec<ScanResponse>)),
)]
pub(crate) async fn scans(
    State(state): State<AppState>,
    Query(query): Query<MonitorQuery>,
) -> Result<Json<Vec<ScanResponse>>, ApiError> {
    let chain_id = chain_filter(&state, query.chain.as_deref())?;
    Ok(Json(
        state
            .scans()
            .jobs(chain_id)
            .iter()
            .map(ScanResponse::from)
            .collect(),
    ))
}

fn event<T: serde::Serialize>(name: &str, payload: &T) -> Event {
    Event::default()
        .event(name)
        .json_data(payload)
        .unwrap_or_else(|_| Event::default().event("error").data("unserialisable"))
}

#[utoipa::path(
    get,
    path = "/monitor/stream",
    tag = "monitor",
    params(MonitorQuery),
    responses((status = OK, description = "Server-sent events: `call` per rpc attempt, `scan` per scan progress, `build` per graph build stage, `lagged` when the client fell behind", content_type = "text/event-stream")),
)]
pub(crate) async fn stream(
    State(state): State<AppState>,
    Query(query): Query<MonitorQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let chain_id = chain_filter(&state, query.chain.as_deref())?;
    let mut calls = state.monitor().subscribe();
    let mut scans = state.scans().subscribe();
    let mut builds = state.builds().subscribe();
    let wanted = move |id: u64| chain_id.is_none_or(|chain_id| chain_id == id);

    let events = async_stream::stream! {
        loop {
            tokio::select! {
                call = calls.recv() => match call {
                    Ok(call) if wanted(call.chain_id) => yield Ok(event("call", &CallResponse::from(&call))),
                    Ok(_) => {}
                    Err(RecvError::Lagged(missed)) => yield Ok(Event::default().event("lagged").data(missed.to_string())),
                    Err(RecvError::Closed) => break,
                },
                job = scans.recv() => match job {
                    Ok(job) if wanted(job.chain_id) => yield Ok(event("scan", &ScanResponse::from(&job))),
                    Ok(_) => {}
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                },
                job = builds.recv() => match job {
                    Ok(job) if wanted(job.chain_id) => yield Ok(event("build", &BuildResponse::from(&job))),
                    Ok(_) => {}
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                },
            }
        }
    };

    Ok(Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
