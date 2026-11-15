use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
};

use domain::account::{TeamId, UserId};

use crate::{
    auth::{CurrentUser, accounts},
    dto::account::{
        AddMemberRequest, MemberResponse, RoleRequest, TeamDetailResponse, TeamRequest,
        TeamResponse, parse_id,
    },
    routes::{ApiError, ErrorResponse},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/teams", get(teams).post(create_team))
        .route(
            "/teams/{team_id}",
            get(team).patch(rename_team).delete(delete_team),
        )
        .route("/teams/{team_id}/members", axum::routing::post(add_member))
        .route(
            "/teams/{team_id}/members/{user_id}",
            patch(change_role).delete(remove_member),
        )
}

#[utoipa::path(
    get,
    path = "/teams",
    tag = "teams",
    responses(
        (status = OK, description = "Teams the account belongs to", body = Vec<TeamResponse>),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorResponse),
    ),
)]
pub(crate) async fn teams(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<Vec<TeamResponse>>, ApiError> {
    let teams = accounts(&state)?.teams.list(&current.user).await?;
    Ok(Json(teams.iter().map(TeamResponse::from).collect()))
}

#[utoipa::path(
    post,
    path = "/teams",
    tag = "teams",
    request_body = TeamRequest,
    responses(
        (status = CREATED, description = "The new team, you own it", body = TeamResponse),
        (status = BAD_REQUEST, description = "The name is not acceptable", body = ErrorResponse),
    ),
)]
pub(crate) async fn create_team(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<TeamRequest>,
) -> Result<(StatusCode, Json<TeamResponse>), ApiError> {
    let membership = accounts(&state)?
        .teams
        .create(&current.user, &request.name)
        .await?;
    Ok((StatusCode::CREATED, Json((&membership).into())))
}

#[utoipa::path(
    get,
    path = "/teams/{team_id}",
    tag = "teams",
    params(("team_id" = String, Path, description = "Team id")),
    responses(
        (status = OK, description = "The team and its members", body = TeamDetailResponse),
        (status = NOT_FOUND, description = "No such team among yours", body = ErrorResponse),
    ),
)]
pub(crate) async fn team(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(team_id): Path<String>,
) -> Result<Json<TeamDetailResponse>, ApiError> {
    let team: TeamId = parse_id(&team_id, "team")?;
    let teams = &accounts(&state)?.teams;
    let membership = teams.membership(&current.user, team).await?;
    let members = teams.members(&current.user, team).await?;
    Ok(Json(TeamDetailResponse::new(&membership, &members)))
}

#[utoipa::path(
    patch,
    path = "/teams/{team_id}",
    tag = "teams",
    params(("team_id" = String, Path, description = "Team id")),
    request_body = TeamRequest,
    responses(
        (status = OK, description = "The renamed team", body = TeamResponse),
        (status = FORBIDDEN, description = "Only owners and admins rename", body = ErrorResponse),
    ),
)]
pub(crate) async fn rename_team(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(team_id): Path<String>,
    Json(request): Json<TeamRequest>,
) -> Result<Json<TeamResponse>, ApiError> {
    let team: TeamId = parse_id(&team_id, "team")?;
    let teams = &accounts(&state)?.teams;
    teams.rename(&current.user, team, &request.name).await?;
    Ok(Json((&teams.membership(&current.user, team).await?).into()))
}

#[utoipa::path(
    delete,
    path = "/teams/{team_id}",
    tag = "teams",
    params(("team_id" = String, Path, description = "Team id")),
    responses(
        (status = NO_CONTENT, description = "The team, its canvases and labels are gone"),
        (status = FORBIDDEN, description = "Only an owner deletes a team", body = ErrorResponse),
    ),
)]
pub(crate) async fn delete_team(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(team_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let team: TeamId = parse_id(&team_id, "team")?;
    accounts(&state)?.teams.delete(&current.user, team).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/teams/{team_id}/members",
    tag = "teams",
    params(("team_id" = String, Path, description = "Team id")),
    request_body = AddMemberRequest,
    responses(
        (status = CREATED, description = "The user is in the team", body = MemberResponse),
        (status = NOT_FOUND, description = "No account with this email", body = ErrorResponse),
        (status = CONFLICT, description = "Already a member", body = ErrorResponse),
    ),
)]
pub(crate) async fn add_member(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(team_id): Path<String>,
    Json(request): Json<AddMemberRequest>,
) -> Result<(StatusCode, Json<MemberResponse>), ApiError> {
    let team: TeamId = parse_id(&team_id, "team")?;
    let member = accounts(&state)?
        .teams
        .add_member(&current.user, team, &request.email, request.role.into())
        .await?;
    Ok((StatusCode::CREATED, Json((&member).into())))
}

#[utoipa::path(
    patch,
    path = "/teams/{team_id}/members/{user_id}",
    tag = "teams",
    params(
        ("team_id" = String, Path, description = "Team id"),
        ("user_id" = String, Path, description = "Member user id"),
    ),
    request_body = RoleRequest,
    responses(
        (status = NO_CONTENT, description = "The role is changed"),
        (status = CONFLICT, description = "The team would lose its last owner", body = ErrorResponse),
    ),
)]
pub(crate) async fn change_role(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((team_id, user_id)): Path<(String, String)>,
    Json(request): Json<RoleRequest>,
) -> Result<StatusCode, ApiError> {
    let team: TeamId = parse_id(&team_id, "team")?;
    let user: UserId = parse_id(&user_id, "user")?;
    accounts(&state)?
        .teams
        .change_role(&current.user, team, user, request.role.into())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/teams/{team_id}/members/{user_id}",
    tag = "teams",
    params(
        ("team_id" = String, Path, description = "Team id"),
        ("user_id" = String, Path, description = "Member user id, your own to leave"),
    ),
    responses(
        (status = NO_CONTENT, description = "The member is out"),
        (status = CONFLICT, description = "The last owner cannot leave", body = ErrorResponse),
    ),
)]
pub(crate) async fn remove_member(
    State(state): State<AppState>,
    current: CurrentUser,
    Path((team_id, user_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let team: TeamId = parse_id(&team_id, "team")?;
    let user: UserId = parse_id(&user_id, "user")?;
    accounts(&state)?
        .teams
        .remove_member(&current.user, team, user)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
