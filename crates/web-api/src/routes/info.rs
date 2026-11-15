use axum::{Json, Router, extract::State, routing::get};

use adapters::eth::networks::known_name;

use crate::{
    auth::CurrentUser,
    dto::info::{ChainTrafficResponse, StorageResponse, UsageResponse},
    routes::{ApiError, ErrorResponse},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/info/usage", get(usage))
}

#[utoipa::path(
    get,
    path = "/info/usage",
    tag = "info",
    responses(
        (status = OK, description = "Disk taken by the indexed transactions and rpc traffic since the server started", body = UsageResponse),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorResponse),
    ),
)]
pub(crate) async fn usage(
    State(state): State<AppState>,
    _current: CurrentUser,
) -> Result<Json<UsageResponse>, ApiError> {
    let storage = match state.storage() {
        Some(repository) => match repository.usage().await {
            Ok(usage) => Some(StorageResponse::new(usage, |chain_id| {
                state
                    .chains()
                    .find(|chain| chain.meta().chain_id == chain_id)
                    .map(|chain| chain.meta().name.clone())
                    .or_else(|| known_name(chain_id).map(str::to_owned))
            })),
            Err(error) => {
                tracing::warn!("could not measure the clickhouse storage: {error}");
                None
            }
        },
        None => None,
    };

    let traffic = state
        .chains()
        .map(|chain| {
            ChainTrafficResponse::new(
                chain.meta().chain_id,
                chain.meta().name.clone(),
                chain.pool().traffic(),
            )
        })
        .collect();

    Ok(Json(UsageResponse::new(
        state.started_at(),
        storage,
        traffic,
    )))
}
