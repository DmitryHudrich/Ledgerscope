use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};

use application::account::{AccountError, Session};

use crate::{
    auth::{CurrentUser, accounts, cleared_cookie, session_cookie, session_token, set_cookie},
    dto::account::{
        AuthConfigResponse, LoginRequest, MeResponse, PasswordRequest, ProfileRequest,
        RegisterRequest,
    },
    routes::{ApiError, ErrorResponse},
    state::{Accounts, AppState},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/config", get(auth_config))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/logout-everywhere", post(logout_everywhere))
        .route("/auth/me", get(me).patch(update_me))
        .route("/auth/password", post(change_password))
}

async fn signed_in(
    accounts: &Accounts,
    session: &Session,
    status: StatusCode,
) -> Result<Response, ApiError> {
    let teams = accounts.teams.list(&session.user).await?;
    let mut response = (status, Json(MeResponse::new(&session.user, &teams))).into_response();
    set_cookie(
        response.headers_mut(),
        session_cookie(
            &session.token,
            accounts.auth.session_ttl().whole_seconds(),
            accounts.cookie_secure,
        ),
    );
    Ok(response)
}

#[utoipa::path(
    get,
    path = "/auth/config",
    tag = "auth",
    responses((status = OK, description = "Whether accounts and sign-up are on", body = AuthConfigResponse)),
)]
pub(crate) async fn auth_config(State(state): State<AppState>) -> Json<AuthConfigResponse> {
    Json(AuthConfigResponse {
        enabled: state.accounts().is_some(),
        registration_open: state
            .accounts()
            .is_some_and(|accounts| accounts.auth.registration_open()),
        personal_rpc: state
            .accounts()
            .is_some_and(|accounts| accounts.rpc_nodes.is_some()),
    })
}

#[utoipa::path(
    post,
    path = "/auth/register",
    tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = CREATED, description = "The account is made and signed in", body = MeResponse),
        (status = BAD_REQUEST, description = "The email, name or password is not acceptable", body = ErrorResponse),
        (status = FORBIDDEN, description = "Registration is closed", body = ErrorResponse),
        (status = CONFLICT, description = "The email is taken", body = ErrorResponse),
    ),
)]
pub(crate) async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<Response, ApiError> {
    let accounts = accounts(&state)?;
    let session = accounts
        .auth
        .register(&request.email, &request.name, &request.password)
        .await?;
    signed_in(accounts, &session, StatusCode::CREATED).await
}

#[utoipa::path(
    post,
    path = "/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = OK, description = "Signed in, the session cookie is set", body = MeResponse),
        (status = UNAUTHORIZED, description = "Wrong email or password", body = ErrorResponse),
        (status = TOO_MANY_REQUESTS, description = "Too many failed attempts", body = ErrorResponse),
    ),
)]
pub(crate) async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    let accounts = accounts(&state)?;
    let key = request.email.trim().to_lowercase();
    accounts.throttle.check(&key)?;

    match accounts.auth.login(&request.email, &request.password).await {
        Ok(session) => {
            accounts.throttle.succeeded(&key);
            signed_in(accounts, &session, StatusCode::OK).await
        }
        Err(AccountError::Invalid(message)) => {
            accounts.throttle.failed(&key);
            Err(ApiError::unauthorized(message))
        }
        Err(error) => Err(error.into()),
    }
}

#[utoipa::path(
    post,
    path = "/auth/logout",
    tag = "auth",
    responses((status = NO_CONTENT, description = "The session is closed and the cookie cleared")),
)]
pub(crate) async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let accounts = accounts(&state)?;
    if let Some(token) = session_token(&headers) {
        accounts.auth.logout(&token).await?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    set_cookie(
        response.headers_mut(),
        cleared_cookie(accounts.cookie_secure),
    );
    Ok(response)
}

#[utoipa::path(
    post,
    path = "/auth/logout-everywhere",
    tag = "auth",
    responses(
        (status = NO_CONTENT, description = "Every session of the account is closed"),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorResponse),
    ),
)]
pub(crate) async fn logout_everywhere(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Response, ApiError> {
    let accounts = accounts(&state)?;
    accounts.auth.logout_everywhere(&current.user).await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    set_cookie(
        response.headers_mut(),
        cleared_cookie(accounts.cookie_secure),
    );
    Ok(response)
}

#[utoipa::path(
    get,
    path = "/auth/me",
    tag = "auth",
    responses(
        (status = OK, description = "The signed-in account and its teams", body = MeResponse),
        (status = UNAUTHORIZED, description = "Not signed in", body = ErrorResponse),
    ),
)]
pub(crate) async fn me(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<MeResponse>, ApiError> {
    let teams = accounts(&state)?.teams.list(&current.user).await?;
    Ok(Json(MeResponse::new(&current.user, &teams)))
}

#[utoipa::path(
    patch,
    path = "/auth/me",
    tag = "auth",
    request_body = ProfileRequest,
    responses(
        (status = OK, description = "The renamed account", body = MeResponse),
        (status = BAD_REQUEST, description = "The name is not acceptable", body = ErrorResponse),
    ),
)]
pub(crate) async fn update_me(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<ProfileRequest>,
) -> Result<Json<MeResponse>, ApiError> {
    let accounts = accounts(&state)?;
    let user = accounts.auth.rename(&current.user, &request.name).await?;
    let teams = accounts.teams.list(&user).await?;
    Ok(Json(MeResponse::new(&user, &teams)))
}

#[utoipa::path(
    post,
    path = "/auth/password",
    tag = "auth",
    request_body = PasswordRequest,
    responses(
        (status = OK, description = "The password is changed, other sessions are closed", body = MeResponse),
        (status = BAD_REQUEST, description = "The current password is wrong or the new one too weak", body = ErrorResponse),
    ),
)]
pub(crate) async fn change_password(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<PasswordRequest>,
) -> Result<Response, ApiError> {
    let accounts = accounts(&state)?;
    let session = accounts
        .auth
        .change_password(&current.user, &request.current, &request.next)
        .await?;
    signed_in(accounts, &session, StatusCode::OK).await
}
