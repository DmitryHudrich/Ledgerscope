use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    routing::{delete, get},
};

use application::account::{CanvasPatch, NewCanvas};
use domain::account::{CanvasId, LinkId, UserId};

use crate::{
    auth::{CurrentUser, accounts},
    dto::account::{
        CanvasDocumentResponse, CanvasResponse, CreateCanvasRequest, LinkResponse, NewLinkResponse,
        PublicCanvasResponse, ShareRequest, ShareResponse, UpdateCanvasRequest, parse_id,
    },
    routes::{ApiError, ErrorResponse},
    state::AppState,
};

const BODY_SLACK: usize = 64 * 1024;

pub fn router(max_document_bytes: usize) -> Router<AppState> {
    Router::new()
        .route("/canvases", get(canvases).post(create_canvas))
        .route(
            "/canvases/{canvas_id}",
            get(canvas).patch(update_canvas).delete(delete_canvas),
        )
        .route("/canvases/{canvas_id}/shares", get(shares).put(share))
        .route("/canvases/{canvas_id}/shares/{user_id}", delete(unshare))
        .route("/canvases/{canvas_id}/links", get(links).post(create_link))
        .route("/canvases/{canvas_id}/links/{link_id}", delete(revoke_link))
        .route("/public/canvases/{token}", get(public_canvas))
        .layer(DefaultBodyLimit::max(max_document_bytes + BODY_SLACK))
}

fn canvas_id(raw: &str) -> Result<CanvasId, ApiError> {
    parse_id(raw, "canvas")
}

#[utoipa::path(
    get,
    path = "/canvases",
    tag = "canvases",
    responses(
        (status = OK, description = "Canvases you own, your teams own or were shared with you", body = Vec<CanvasResponse>),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorResponse),
    ),
)]
pub(crate) async fn canvases(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<CanvasResponse>>, ApiError> {
    let canvases = accounts(&state)?.canvases.list(&current.user).await?;
    Ok(Json(canvases.iter().map(CanvasResponse::from).collect()))
}

#[utoipa::path(
    post,
    path = "/canvases",
    tag = "canvases",
    request_body = CreateCanvasRequest,
    responses(
        (status = CREATED, description = "The saved canvas", body = CanvasResponse),
        (status = BAD_REQUEST, description = "The title or document is not acceptable", body = ErrorResponse),
        (status = FORBIDDEN, description = "Team viewers cannot create canvases", body = ErrorResponse),
    ),
)]
pub(crate) async fn create_canvas(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<CreateCanvasRequest>,
) -> Result<(StatusCode, Json<CanvasResponse>), ApiError> {
    let owner = request.owner.resolve(&current.user)?;
    let canvas = accounts(&state)?
        .canvases
        .create(
            &current.user,
            NewCanvas {
                owner,
                title: request.title,
                chain_id: request.chain_id,
                document: request.document,
            },
        )
        .await?;
    Ok((StatusCode::CREATED, Json((&canvas).into())))
}

#[utoipa::path(
    get,
    path = "/canvases/{canvas_id}",
    tag = "canvases",
    params(("canvas_id" = String, Path, description = "Canvas id")),
    responses(
        (status = OK, description = "The canvas with its document", body = CanvasDocumentResponse),
        (status = NOT_FOUND, description = "No such canvas you can see", body = ErrorResponse),
    ),
)]
pub(crate) async fn canvas(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(raw): Path<String>,
) -> Result<Json<CanvasDocumentResponse>, ApiError> {
    let (canvas, document) = accounts(&state)?
        .canvases
        .open(&current.user, canvas_id(&raw)?)
        .await?;
    Ok(Json(CanvasDocumentResponse::new(&canvas, document)))
}

#[utoipa::path(
    patch,
    path = "/canvases/{canvas_id}",
    tag = "canvases",
    params(("canvas_id" = String, Path, description = "Canvas id")),
    request_body = UpdateCanvasRequest,
    responses(
        (status = OK, description = "The canvas at its next version", body = CanvasResponse),
        (status = FORBIDDEN, description = "You can only view it", body = ErrorResponse),
        (status = CONFLICT, description = "Someone saved it since your version", body = ErrorResponse),
    ),
)]
pub(crate) async fn update_canvas(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(raw): Path<String>,
    Json(request): Json<UpdateCanvasRequest>,
) -> Result<Json<CanvasResponse>, ApiError> {
    let canvas = accounts(&state)?
        .canvases
        .update(
            &current.user,
            canvas_id(&raw)?,
            CanvasPatch {
                title: request.title,
                chain_id: request.chain_id,
                document: request.document,
            },
            request.version,
        )
        .await?;
    Ok(Json((&canvas).into()))
}

#[utoipa::path(
    delete,
    path = "/canvases/{canvas_id}",
    tag = "canvases",
    params(("canvas_id" = String, Path, description = "Canvas id")),
    responses(
        (status = NO_CONTENT, description = "The canvas is gone"),
        (status = FORBIDDEN, description = "Only who manages it deletes it", body = ErrorResponse),
    ),
)]
pub(crate) async fn delete_canvas(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(raw): Path<String>,
) -> Result<StatusCode, ApiError> {
    accounts(&state)?
        .canvases
        .delete(&current.user, canvas_id(&raw)?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/canvases/{canvas_id}/shares",
    tag = "canvases",
    params(("canvas_id" = String, Path, description = "Canvas id")),
    responses((status = OK, description = "Who the canvas is shared with", body = Vec<ShareResponse>)),
)]
pub(crate) async fn shares(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(raw): Path<String>,
) -> Result<Json<Vec<ShareResponse>>, ApiError> {
    let shares = accounts(&state)?
        .canvases
        .shares(&current.user, canvas_id(&raw)?)
        .await?;
    Ok(Json(shares.iter().map(ShareResponse::from).collect()))
}

#[utoipa::path(
    put,
    path = "/canvases/{canvas_id}/shares",
    tag = "canvases",
    params(("canvas_id" = String, Path, description = "Canvas id")),
    request_body = ShareRequest,
    responses(
        (status = OK, description = "The user can open the canvas now", body = ShareResponse),
        (status = NOT_FOUND, description = "No account with this email", body = ErrorResponse),
    ),
)]
pub(crate) async fn share(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(raw): Path<String>,
    Json(request): Json<ShareRequest>,
) -> Result<Json<ShareResponse>, ApiError> {
    let share = accounts(&state)?
        .canvases
        .share(
            &current.user,
            canvas_id(&raw)?,
            &request.email,
            request.access.into(),
        )
        .await?;
    Ok(Json((&share).into()))
}

#[utoipa::path(
    delete,
    path = "/canvases/{canvas_id}/shares/{user_id}",
    tag = "canvases",
    params(
        ("canvas_id" = String, Path, description = "Canvas id"),
        ("user_id" = String, Path, description = "The user to take access from, your own to drop it"),
    ),
    responses((status = NO_CONTENT, description = "The share is gone")),
)]
pub(crate) async fn unshare(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((raw, user_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let user: UserId = parse_id(&user_id, "user")?;
    accounts(&state)?
        .canvases
        .unshare(&current.user, canvas_id(&raw)?, user)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/canvases/{canvas_id}/links",
    tag = "canvases",
    params(("canvas_id" = String, Path, description = "Canvas id")),
    responses((status = OK, description = "Public read-only links, live and revoked", body = Vec<LinkResponse>)),
)]
pub(crate) async fn links(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(raw): Path<String>,
) -> Result<Json<Vec<LinkResponse>>, ApiError> {
    let links = accounts(&state)?
        .canvases
        .links(&current.user, canvas_id(&raw)?)
        .await?;
    Ok(Json(links.iter().map(LinkResponse::from).collect()))
}

#[utoipa::path(
    post,
    path = "/canvases/{canvas_id}/links",
    tag = "canvases",
    params(("canvas_id" = String, Path, description = "Canvas id")),
    responses((status = CREATED, description = "A new public link, its token is shown once", body = NewLinkResponse)),
)]
pub(crate) async fn create_link(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(raw): Path<String>,
) -> Result<(StatusCode, Json<NewLinkResponse>), ApiError> {
    let (link, token) = accounts(&state)?
        .canvases
        .create_link(&current.user, canvas_id(&raw)?)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(NewLinkResponse {
            link: (&link).into(),
            token,
        }),
    ))
}

#[utoipa::path(
    delete,
    path = "/canvases/{canvas_id}/links/{link_id}",
    tag = "canvases",
    params(
        ("canvas_id" = String, Path, description = "Canvas id"),
        ("link_id" = String, Path, description = "Link id"),
    ),
    responses((status = NO_CONTENT, description = "The link stops working")),
)]
pub(crate) async fn revoke_link(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((raw, link_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let link: LinkId = parse_id(&link_id, "link")?;
    accounts(&state)?
        .canvases
        .revoke_link(&current.user, canvas_id(&raw)?, link)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/public/canvases/{token}",
    tag = "canvases",
    params(("token" = String, Path, description = "The token of a public link")),
    responses(
        (status = OK, description = "The canvas, read-only, no account needed", body = PublicCanvasResponse),
        (status = NOT_FOUND, description = "The link is unknown or revoked", body = ErrorResponse),
    ),
)]
pub(crate) async fn public_canvas(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<PublicCanvasResponse>, ApiError> {
    let canvas = accounts(&state)?.canvases.open_public(&token).await?;
    Ok(Json(PublicCanvasResponse::new(canvas)))
}
