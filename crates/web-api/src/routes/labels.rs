use std::{collections::HashMap, str::FromStr};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch},
};
use serde::Deserialize;
use utoipa::IntoParams;

use application::account::NewLabel;
use domain::{
    account::LabelId,
    eth::{AddressLabel, EthAddress},
};

use crate::{
    auth::{CurrentUser, accounts},
    dto::{
        account::{CreateLabelRequest, UpdateLabelRequest, UserLabelResponse, parse_id},
        parse_address,
    },
    routes::{ApiError, ErrorResponse},
    state::AppState,
};

const MAX_QUERIED_ADDRESSES: usize = 5_000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/labels", get(labels).post(create_label))
        .route(
            "/labels/{label_id}",
            patch(update_label).delete(delete_label),
        )
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LabelsQuery {
    chain_id: Option<u64>,
    addresses: Option<String>,
}

fn addresses(raw: Option<&str>) -> Result<Option<Vec<EthAddress>>, ApiError> {
    let Some(raw) = raw.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Ok(None);
    };
    let parsed = raw
        .split(',')
        .map(str::trim)
        .filter(|address| !address.is_empty())
        .map(|address| {
            parse_address(address)
                .map_err(|message| ApiError::new(StatusCode::BAD_REQUEST, message))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if parsed.len() > MAX_QUERIED_ADDRESSES {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("at most {MAX_QUERIED_ADDRESSES} addresses at a time"),
        ));
    }
    Ok(Some(parsed))
}

#[utoipa::path(
    get,
    path = "/labels",
    tag = "labels",
    params(LabelsQuery),
    responses(
        (status = OK, description = "Your own labels and your teams' labels", body = Vec<UserLabelResponse>),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorResponse),
    ),
)]
pub(crate) async fn labels(
    State(state): State<AppState>,
    current: CurrentUser,
    Query(query): Query<LabelsQuery>,
) -> Result<Json<Vec<UserLabelResponse>>, ApiError> {
    let addresses = addresses(query.addresses.as_deref())?;
    let labels = accounts(&state)?
        .labels
        .visible(&current.user, query.chain_id, addresses.as_deref())
        .await?;
    Ok(Json(labels.iter().map(UserLabelResponse::from).collect()))
}

#[utoipa::path(
    post,
    path = "/labels",
    tag = "labels",
    request_body = CreateLabelRequest,
    responses(
        (status = CREATED, description = "The label is on the address", body = UserLabelResponse),
        (status = CONFLICT, description = "The address already carries that label", body = ErrorResponse),
        (status = FORBIDDEN, description = "Team viewers cannot add labels", body = ErrorResponse),
    ),
)]
pub(crate) async fn create_label(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<CreateLabelRequest>,
) -> Result<(StatusCode, Json<UserLabelResponse>), ApiError> {
    let owner = request.owner.resolve(&current.user)?;
    let address = parse_address(&request.address)
        .map_err(|message| ApiError::new(StatusCode::BAD_REQUEST, message))?;
    let label = accounts(&state)?
        .labels
        .create(
            &current.user,
            NewLabel {
                owner,
                chain_id: request.chain_id,
                address,
                value: request.value,
                note: request.note,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json((&label).into())))
}

#[utoipa::path(
    patch,
    path = "/labels/{label_id}",
    tag = "labels",
    params(("label_id" = String, Path, description = "Label id")),
    request_body = UpdateLabelRequest,
    responses((status = OK, description = "The changed label", body = UserLabelResponse)),
)]
pub(crate) async fn update_label(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(raw): Path<String>,
    Json(request): Json<UpdateLabelRequest>,
) -> Result<Json<UserLabelResponse>, ApiError> {
    let id = LabelId::from_str(&raw)
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, format!("{raw} is not a label id")))?;
    let label = accounts(&state)?
        .labels
        .update(&current.user, id, &request.value, &request.note)
        .await?;
    Ok(Json((&label).into()))
}

#[utoipa::path(
    delete,
    path = "/labels/{label_id}",
    tag = "labels",
    params(("label_id" = String, Path, description = "Label id")),
    responses((status = NO_CONTENT, description = "The label is gone")),
)]
pub(crate) async fn delete_label(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(raw): Path<String>,
) -> Result<StatusCode, ApiError> {
    let id: LabelId = parse_id(&raw, "label")?;
    accounts(&state)?.labels.delete(&current.user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn graph_labels(
    state: &AppState,
    user: Option<&application::account::User>,
    chain_id: u64,
    addresses: Vec<EthAddress>,
) -> HashMap<EthAddress, Vec<AddressLabel>> {
    let (Some(accounts), Some(user)) = (state.accounts(), user) else {
        return HashMap::new();
    };
    match accounts.labels.for_graph(user, chain_id, &addresses).await {
        Ok(labels) => labels,
        Err(error) => {
            tracing::warn!("the graph goes out without your labels: {error}");
            HashMap::new()
        }
    }
}
