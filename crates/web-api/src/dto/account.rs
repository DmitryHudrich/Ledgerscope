use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use utoipa::ToSchema;

use application::account::{
    CanvasShare, Member, Membership, PublicLink, User, VisibleCanvas, VisibleLabel,
};
use domain::account::{LabelAccess, Owner, ShareAccess, TeamId, TeamRole};

use crate::routes::ApiError;

fn stamp(at: OffsetDateTime) -> String {
    at.format(&Rfc3339).unwrap_or_default()
}

pub fn parse_id<T: std::str::FromStr>(raw: &str, what: &str) -> Result<T, ApiError> {
    raw.parse().map_err(|_| {
        ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            format!("{raw} is not a {what} id"),
        )
    })
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoleDto {
    Owner,
    Admin,
    Editor,
    Viewer,
}

impl From<RoleDto> for TeamRole {
    fn from(role: RoleDto) -> Self {
        match role {
            RoleDto::Owner => Self::Owner,
            RoleDto::Admin => Self::Admin,
            RoleDto::Editor => Self::Editor,
            RoleDto::Viewer => Self::Viewer,
        }
    }
}

impl From<TeamRole> for RoleDto {
    fn from(role: TeamRole) -> Self {
        match role {
            TeamRole::Owner => Self::Owner,
            TeamRole::Admin => Self::Admin,
            TeamRole::Editor => Self::Editor,
            TeamRole::Viewer => Self::Viewer,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShareDto {
    View,
    Edit,
}

impl From<ShareDto> for ShareAccess {
    fn from(access: ShareDto) -> Self {
        match access {
            ShareDto::View => Self::View,
            ShareDto::Edit => Self::Edit,
        }
    }
}

impl From<ShareAccess> for ShareDto {
    fn from(access: ShareAccess) -> Self {
        match access {
            ShareAccess::View => Self::View,
            ShareAccess::Edit => Self::Edit,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OwnerDto {
    User,
    Team { team_id: String },
}

impl OwnerDto {
    pub fn resolve(&self, user: &User) -> Result<Owner, ApiError> {
        match self {
            Self::User => Ok(Owner::User(user.id)),
            Self::Team { team_id } => Ok(Owner::Team(parse_id::<TeamId>(team_id, "team")?)),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct OwnerResponse {
    kind: &'static str,
    id: String,
    team_name: Option<String>,
}

impl OwnerResponse {
    fn new(owner: Owner, team_name: Option<String>) -> Self {
        match owner {
            Owner::User(user) => Self {
                kind: "user",
                id: user.to_string(),
                team_name: None,
            },
            Owner::Team(team) => Self {
                kind: "team",
                id: team.to_string(),
                team_name,
            },
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct AuthConfigResponse {
    pub enabled: bool,
    pub registration_open: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub email: String,
    pub name: String,
    pub password: String,
}

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ProfileRequest {
    pub name: String,
}

#[derive(Deserialize, ToSchema)]
pub struct PasswordRequest {
    pub current: String,
    pub next: String,
}

#[derive(Serialize, ToSchema)]
pub struct UserResponse {
    id: String,
    email: String,
    name: String,
    created_at: String,
}

impl From<&User> for UserResponse {
    fn from(user: &User) -> Self {
        Self {
            id: user.id.to_string(),
            email: user.email.to_string(),
            name: user.name.to_string(),
            created_at: stamp(user.created_at),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct MeResponse {
    user: UserResponse,
    teams: Vec<TeamResponse>,
}

impl MeResponse {
    pub fn new(user: &User, teams: &[Membership]) -> Self {
        Self {
            user: user.into(),
            teams: teams.iter().map(TeamResponse::from).collect(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct TeamResponse {
    id: String,
    name: String,
    role: RoleDto,
    members: usize,
    created_at: String,
}

impl From<&Membership> for TeamResponse {
    fn from(membership: &Membership) -> Self {
        Self {
            id: membership.team.id.to_string(),
            name: membership.team.name.to_string(),
            role: membership.role.into(),
            members: membership.members,
            created_at: stamp(membership.team.created_at),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct TeamDetailResponse {
    team: TeamResponse,
    members: Vec<MemberResponse>,
}

impl TeamDetailResponse {
    pub fn new(membership: &Membership, members: &[Member]) -> Self {
        Self {
            team: membership.into(),
            members: members.iter().map(MemberResponse::from).collect(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct MemberResponse {
    user: UserResponse,
    role: RoleDto,
    joined_at: String,
}

impl From<&Member> for MemberResponse {
    fn from(member: &Member) -> Self {
        Self {
            user: (&member.user).into(),
            role: member.role.into(),
            joined_at: stamp(member.joined_at),
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct TeamRequest {
    pub name: String,
}

#[derive(Deserialize, ToSchema)]
pub struct AddMemberRequest {
    pub email: String,
    pub role: RoleDto,
}

#[derive(Deserialize, ToSchema)]
pub struct RoleRequest {
    pub role: RoleDto,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateCanvasRequest {
    pub title: String,
    pub chain_id: u64,
    pub owner: OwnerDto,
    #[schema(value_type = Object)]
    pub document: serde_json::Value,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateCanvasRequest {
    pub version: i64,
    pub title: Option<String>,
    pub chain_id: Option<u64>,
    #[schema(value_type = Option<Object>)]
    pub document: Option<serde_json::Value>,
}

#[derive(Serialize, ToSchema)]
pub struct CanvasResponse {
    id: String,
    title: String,
    chain_id: u64,
    version: i64,
    owner: OwnerResponse,
    access: &'static str,
    created_by: String,
    updated_by: String,
    created_at: String,
    updated_at: String,
}

impl From<&VisibleCanvas> for CanvasResponse {
    fn from(canvas: &VisibleCanvas) -> Self {
        let summary = &canvas.summary;
        Self {
            id: summary.id.to_string(),
            title: summary.title.to_string(),
            chain_id: summary.chain_id,
            version: summary.version,
            owner: OwnerResponse::new(summary.owner, canvas.team_name.clone()),
            access: canvas.access.as_str(),
            created_by: summary.created_by.to_string(),
            updated_by: summary.updated_by.to_string(),
            created_at: stamp(summary.created_at),
            updated_at: stamp(summary.updated_at),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct CanvasDocumentResponse {
    canvas: CanvasResponse,
    #[schema(value_type = Object)]
    document: serde_json::Value,
}

impl CanvasDocumentResponse {
    pub fn new(canvas: &VisibleCanvas, document: serde_json::Value) -> Self {
        Self {
            canvas: canvas.into(),
            document,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct PublicCanvasResponse {
    pub title: String,
    pub chain_id: u64,
    pub updated_at: String,
    #[schema(value_type = Object)]
    pub document: serde_json::Value,
}

impl PublicCanvasResponse {
    pub fn new(canvas: application::account::Canvas) -> Self {
        Self {
            title: canvas.summary.title.to_string(),
            chain_id: canvas.summary.chain_id,
            updated_at: stamp(canvas.summary.updated_at),
            document: canvas.document,
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct ShareRequest {
    pub email: String,
    pub access: ShareDto,
}

#[derive(Serialize, ToSchema)]
pub struct ShareResponse {
    user: UserResponse,
    access: ShareDto,
    created_at: String,
}

impl From<&CanvasShare> for ShareResponse {
    fn from(share: &CanvasShare) -> Self {
        Self {
            user: (&share.user).into(),
            access: share.access.into(),
            created_at: stamp(share.created_at),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct LinkResponse {
    id: String,
    created_by: String,
    created_at: String,
    revoked_at: Option<String>,
}

impl From<&PublicLink> for LinkResponse {
    fn from(link: &PublicLink) -> Self {
        Self {
            id: link.id.to_string(),
            created_by: link.created_by.to_string(),
            created_at: stamp(link.created_at),
            revoked_at: link.revoked_at.map(stamp),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct NewLinkResponse {
    pub link: LinkResponse,
    pub token: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateLabelRequest {
    pub chain_id: u64,
    pub address: String,
    pub value: String,
    #[serde(default)]
    pub note: String,
    pub owner: OwnerDto,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateLabelRequest {
    pub value: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Serialize, ToSchema)]
pub struct UserLabelResponse {
    id: String,
    chain_id: u64,
    address: String,
    value: String,
    note: String,
    owner: OwnerResponse,
    writable: bool,
    created_by: String,
    created_at: String,
    updated_at: String,
}

impl From<&VisibleLabel> for UserLabelResponse {
    fn from(visible: &VisibleLabel) -> Self {
        let label = &visible.label;
        Self {
            id: label.id.to_string(),
            chain_id: label.chain_id,
            address: label.address.to_string(),
            value: label.value.to_string(),
            note: label.note.clone(),
            owner: OwnerResponse::new(label.owner, label.team_name.clone()),
            writable: visible.access == LabelAccess::Write,
            created_by: label.created_by.to_string(),
            created_at: stamp(label.created_at),
            updated_at: stamp(label.updated_at),
        }
    }
}
