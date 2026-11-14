use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get},
};
use serde::Deserialize;
use utoipa::IntoParams;

use application::account::RpcNodeService;
use domain::account::RpcNodeId;

use crate::{
    auth::{CurrentUser, accounts},
    dto::account::{AddRpcNodeRequest, RpcNodeResponse, parse_id},
    routes::{ApiError, ErrorResponse},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/rpc-nodes", get(rpc_nodes).post(add_rpc_node))
        .route("/rpc-nodes/{node_id}", delete(delete_rpc_node))
}

fn service(state: &AppState) -> Result<&RpcNodeService, ApiError> {
    accounts(state)?
        .rpc_nodes
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("personal rpc nodes are off on this server"))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RpcNodesQuery {
    chain_id: Option<u64>,
}

#[utoipa::path(
    get,
    path = "/rpc-nodes",
    tag = "rpc-nodes",
    params(RpcNodesQuery),
    responses(
        (status = OK, description = "Your own rpc nodes, keys hidden", body = Vec<RpcNodeResponse>),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorResponse),
        (status = SERVICE_UNAVAILABLE, description = "Personal rpc nodes are off", body = ErrorResponse),
    ),
)]
pub(crate) async fn rpc_nodes(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(query): Query<RpcNodesQuery>,
) -> Result<Json<Vec<RpcNodeResponse>>, ApiError> {
    let nodes = service(&state)?.list(&current.user, query.chain_id).await?;
    Ok(Json(nodes.iter().map(RpcNodeResponse::from).collect()))
}

#[utoipa::path(
    post,
    path = "/rpc-nodes",
    tag = "rpc-nodes",
    request_body = AddRpcNodeRequest,
    responses(
        (status = CREATED, description = "The node answered as the chain and is yours now", body = RpcNodeResponse),
        (status = BAD_REQUEST, description = "The URL is not allowed or the node did not pass the check", body = ErrorResponse),
        (status = CONFLICT, description = "You already have this node on that chain", body = ErrorResponse),
    ),
)]
pub(crate) async fn add_rpc_node(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<AddRpcNodeRequest>,
) -> Result<(StatusCode, Json<RpcNodeResponse>), ApiError> {
    let node = service(&state)?
        .add(&current.user, request.chain_id, &request.url)
        .await?;
    Ok((StatusCode::CREATED, Json((&node).into())))
}

#[utoipa::path(
    delete,
    path = "/rpc-nodes/{node_id}",
    tag = "rpc-nodes",
    params(("node_id" = String, Path, description = "Rpc node id")),
    responses(
        (status = NO_CONTENT, description = "The node is gone"),
        (status = NOT_FOUND, description = "No such node of yours", body = ErrorResponse),
    ),
)]
pub(crate) async fn delete_rpc_node(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(raw): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id: RpcNodeId = parse_id(&raw, "rpc node")?;
    service(&state)?.delete(&current.user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
