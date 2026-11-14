use time::OffsetDateTime;

use domain::{
    account::{
        CanvasAccess, CanvasId, CanvasTitle, DisplayName, Email, LabelAccess, LabelId, LabelValue,
        LinkId, Owner, RpcNodeId, ShareAccess, TeamId, TeamName, TeamRole, UserId,
    },
    eth::EthAddress,
};

#[derive(Clone, Debug)]
pub struct User {
    pub id: UserId,
    pub email: Email,
    pub name: DisplayName,
    pub created_at: OffsetDateTime,
}

#[derive(Clone, Debug)]
pub struct Credentials {
    pub user: User,
    pub password_hash: String,
}

#[derive(Clone, Debug)]
pub struct Team {
    pub id: TeamId,
    pub name: TeamName,
    pub created_at: OffsetDateTime,
}

#[derive(Clone, Debug)]
pub struct Membership {
    pub team: Team,
    pub role: TeamRole,
    pub members: usize,
}

#[derive(Clone, Debug)]
pub struct Member {
    pub user: User,
    pub role: TeamRole,
    pub joined_at: OffsetDateTime,
}

#[derive(Clone, Debug)]
pub struct CanvasSummary {
    pub id: CanvasId,
    pub owner: Owner,
    pub title: CanvasTitle,
    pub chain_id: u64,
    pub version: i64,
    pub created_by: UserId,
    pub updated_by: UserId,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Clone, Debug)]
pub struct Canvas {
    pub summary: CanvasSummary,
    pub document: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct CanvasRecord {
    pub summary: CanvasSummary,
    pub team_name: Option<String>,
    pub team_role: Option<TeamRole>,
    pub share: Option<ShareAccess>,
}

#[derive(Clone, Debug)]
pub struct VisibleCanvas {
    pub summary: CanvasSummary,
    pub team_name: Option<String>,
    pub access: CanvasAccess,
}

#[derive(Clone, Debug)]
pub struct CanvasShare {
    pub user: User,
    pub access: ShareAccess,
    pub created_at: OffsetDateTime,
}

#[derive(Clone, Debug)]
pub struct PublicLink {
    pub id: LinkId,
    pub canvas: CanvasId,
    pub created_by: UserId,
    pub created_at: OffsetDateTime,
    pub revoked_at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug)]
pub struct UserLabel {
    pub id: LabelId,
    pub owner: Owner,
    pub team_name: Option<String>,
    pub team_role: Option<TeamRole>,
    pub chain_id: u64,
    pub address: EthAddress,
    pub value: LabelValue,
    pub note: String,
    pub created_by: UserId,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl UserLabel {
    pub fn access(&self, viewer: UserId) -> Option<LabelAccess> {
        domain::account::label_access(viewer, self.owner, self.team_role)
    }
}

#[derive(Clone, Debug)]
pub struct RpcNode {
    pub id: RpcNodeId,
    pub user_id: UserId,
    pub chain_id: u64,
    pub url: String,
    pub created_at: OffsetDateTime,
}
