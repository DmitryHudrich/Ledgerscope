use std::io;

use time::OffsetDateTime;

use domain::{
    account::{
        CanvasId, CanvasTitle, DisplayName, Email, LabelId, LabelValue, LinkId, RpcNodeId,
        ShareAccess, TeamId, TeamName, TeamRole, UserId,
    },
    eth::EthAddress,
};

use crate::account::{
    StoreError,
    model::{
        Canvas, CanvasRecord, CanvasShare, CanvasSummary, Credentials, Member, Membership,
        PublicLink, RpcNode, Team, User, UserLabel,
    },
};

#[async_trait::async_trait]
pub trait UserStore: Send + Sync {
    async fn insert(&self, user: &User, password_hash: &str) -> Result<(), StoreError>;

    async fn credentials(&self, email: &Email) -> Result<Option<Credentials>, io::Error>;

    async fn credentials_of(&self, id: UserId) -> Result<Option<Credentials>, io::Error>;

    async fn by_email(&self, email: &Email) -> Result<Option<User>, io::Error>;

    async fn rename(&self, id: UserId, name: &DisplayName) -> Result<(), io::Error>;

    async fn set_password_hash(&self, id: UserId, password_hash: &str) -> Result<(), io::Error>;
}

#[async_trait::async_trait]
pub trait SessionStore: Send + Sync {
    async fn insert(
        &self,
        digest: &[u8],
        user: UserId,
        expires_at: OffsetDateTime,
    ) -> Result<(), io::Error>;

    async fn user(&self, digest: &[u8], now: OffsetDateTime) -> Result<Option<User>, io::Error>;

    async fn delete(&self, digest: &[u8]) -> Result<(), io::Error>;

    async fn delete_all(&self, user: UserId) -> Result<(), io::Error>;

    async fn purge_expired(&self, now: OffsetDateTime) -> Result<u64, io::Error>;
}

#[async_trait::async_trait]
pub trait TeamStore: Send + Sync {
    async fn insert(&self, team: &Team, owner: UserId) -> Result<(), StoreError>;

    async fn memberships(&self, user: UserId) -> Result<Vec<Membership>, io::Error>;

    async fn membership(&self, team: TeamId, user: UserId)
    -> Result<Option<Membership>, io::Error>;

    async fn members(&self, team: TeamId) -> Result<Vec<Member>, io::Error>;

    async fn add_member(
        &self,
        team: TeamId,
        user: UserId,
        role: TeamRole,
    ) -> Result<(), StoreError>;

    async fn set_role(&self, team: TeamId, user: UserId, role: TeamRole) -> Result<(), StoreError>;

    async fn remove_member(&self, team: TeamId, user: UserId) -> Result<(), StoreError>;

    async fn rename(&self, team: TeamId, name: &TeamName) -> Result<(), io::Error>;

    async fn delete(&self, team: TeamId) -> Result<(), io::Error>;
}

#[derive(Clone, Debug, Default)]
pub struct CanvasChange {
    pub title: Option<CanvasTitle>,
    pub chain_id: Option<u64>,
    pub document: Option<serde_json::Value>,
}

#[async_trait::async_trait]
pub trait CanvasStore: Send + Sync {
    async fn insert(&self, canvas: &Canvas) -> Result<(), StoreError>;

    async fn record(&self, id: CanvasId, viewer: UserId)
    -> Result<Option<CanvasRecord>, io::Error>;

    async fn document(&self, id: CanvasId) -> Result<Option<serde_json::Value>, io::Error>;

    async fn list(&self, viewer: UserId) -> Result<Vec<CanvasRecord>, io::Error>;

    async fn update(
        &self,
        id: CanvasId,
        change: &CanvasChange,
        expected_version: i64,
        by: UserId,
    ) -> Result<CanvasSummary, StoreError>;

    async fn delete(&self, id: CanvasId) -> Result<(), io::Error>;

    async fn shares(&self, id: CanvasId) -> Result<Vec<CanvasShare>, io::Error>;

    async fn put_share(
        &self,
        id: CanvasId,
        user: UserId,
        access: ShareAccess,
    ) -> Result<(), io::Error>;

    async fn remove_share(&self, id: CanvasId, user: UserId) -> Result<bool, io::Error>;

    async fn insert_link(&self, link: &PublicLink, digest: &[u8]) -> Result<(), io::Error>;

    async fn links(&self, id: CanvasId) -> Result<Vec<PublicLink>, io::Error>;

    async fn revoke_link(
        &self,
        id: CanvasId,
        link: LinkId,
        now: OffsetDateTime,
    ) -> Result<bool, io::Error>;

    async fn by_link(&self, digest: &[u8]) -> Result<Option<Canvas>, io::Error>;
}

#[async_trait::async_trait]
pub trait LabelStore: Send + Sync {
    async fn visible(
        &self,
        viewer: UserId,
        chain_id: Option<u64>,
        addresses: Option<&[EthAddress]>,
    ) -> Result<Vec<UserLabel>, io::Error>;

    async fn label(&self, id: LabelId, viewer: UserId) -> Result<Option<UserLabel>, io::Error>;

    async fn insert(&self, label: &UserLabel) -> Result<(), StoreError>;

    async fn update(
        &self,
        id: LabelId,
        value: &LabelValue,
        note: &str,
        now: OffsetDateTime,
    ) -> Result<(), StoreError>;

    async fn delete(&self, id: LabelId) -> Result<(), io::Error>;
}

#[async_trait::async_trait]
pub trait RpcNodeStore: Send + Sync {
    async fn list(&self, user: UserId, chain_id: Option<u64>) -> Result<Vec<RpcNode>, io::Error>;

    async fn insert(&self, node: &RpcNode) -> Result<(), StoreError>;

    async fn delete(&self, id: RpcNodeId, user: UserId) -> Result<bool, io::Error>;
}

#[async_trait::async_trait]
pub trait RpcNodeProbe: Send + Sync {
    async fn probe(&self, chain_id: u64, url: &str) -> Result<String, String>;
}

pub trait PasswordHasher: Send + Sync {
    fn hash(&self, password: &str) -> Result<String, io::Error>;

    fn verify(&self, password: &str, hash: &str) -> bool;
}

pub trait TokenMint: Send + Sync {
    fn mint(&self) -> String;

    fn digest(&self, token: &str) -> Vec<u8>;
}
