use std::sync::Arc;

use time::OffsetDateTime;

use domain::account::{
    CanvasAccess, CanvasId, CanvasTitle, Email, LinkId, Owner, ShareAccess, UserId, canvas_access,
};

use crate::account::{
    AccountError,
    model::{Canvas, CanvasRecord, CanvasShare, CanvasSummary, PublicLink, User, VisibleCanvas},
    ports::{CanvasChange, CanvasStore, TeamStore, TokenMint, UserStore},
};

#[derive(Clone, Debug)]
pub struct NewCanvas {
    pub owner: Owner,
    pub title: String,
    pub chain_id: u64,
    pub document: serde_json::Value,
}

#[derive(Clone, Debug, Default)]
pub struct CanvasPatch {
    pub title: Option<String>,
    pub chain_id: Option<u64>,
    pub document: Option<serde_json::Value>,
}

pub struct CanvasService {
    canvases: Arc<dyn CanvasStore>,
    teams: Arc<dyn TeamStore>,
    users: Arc<dyn UserStore>,
    tokens: Arc<dyn TokenMint>,
    max_document_bytes: usize,
}

impl CanvasService {
    pub fn new(
        canvases: Arc<dyn CanvasStore>,
        teams: Arc<dyn TeamStore>,
        users: Arc<dyn UserStore>,
        tokens: Arc<dyn TokenMint>,
        max_document_bytes: usize,
    ) -> Self {
        Self {
            canvases,
            teams,
            users,
            tokens,
            max_document_bytes,
        }
    }

    pub fn max_document_bytes(&self) -> usize {
        self.max_document_bytes
    }

    pub async fn create(
        &self,
        actor: &User,
        new: NewCanvas,
    ) -> Result<VisibleCanvas, AccountError> {
        let title = CanvasTitle::parse(&new.title)?;
        self.check_document(&new.document)?;

        let (team_name, access) = match new.owner {
            Owner::User(user) if user == actor.id => (None, CanvasAccess::Manage),
            Owner::User(_) => {
                return Err(AccountError::forbidden(
                    "a personal canvas belongs to you alone",
                ));
            }
            Owner::Team(team) => {
                let membership = self
                    .teams
                    .membership(team, actor.id)
                    .await?
                    .ok_or(AccountError::NotFound("team"))?;
                if !membership.role.can_edit() {
                    return Err(AccountError::forbidden(
                        "team viewers cannot create canvases",
                    ));
                }
                let access = canvas_access(actor.id, new.owner, Some(membership.role), None)
                    .unwrap_or(CanvasAccess::View);
                (Some(membership.team.name.into_inner()), access)
            }
        };

        let now = OffsetDateTime::now_utc();
        let summary = CanvasSummary {
            id: CanvasId::new(),
            owner: new.owner,
            title,
            chain_id: new.chain_id,
            version: 1,
            created_by: actor.id,
            updated_by: actor.id,
            created_at: now,
            updated_at: now,
        };
        self.canvases
            .insert(&Canvas {
                summary: summary.clone(),
                document: new.document,
            })
            .await?;

        Ok(VisibleCanvas {
            summary,
            team_name,
            access,
        })
    }

    pub async fn list(&self, actor: &User) -> Result<Vec<VisibleCanvas>, AccountError> {
        Ok(self
            .canvases
            .list(actor.id)
            .await?
            .into_iter()
            .filter_map(|record| visible(actor.id, record))
            .collect())
    }

    pub async fn open(
        &self,
        actor: &User,
        id: CanvasId,
    ) -> Result<(VisibleCanvas, serde_json::Value), AccountError> {
        let canvas = self.reach(actor, id, CanvasAccess::View).await?;
        let document = self
            .canvases
            .document(id)
            .await?
            .ok_or(AccountError::NotFound("canvas"))?;
        Ok((canvas, document))
    }

    pub async fn update(
        &self,
        actor: &User,
        id: CanvasId,
        patch: CanvasPatch,
        expected_version: i64,
    ) -> Result<VisibleCanvas, AccountError> {
        let canvas = self.reach(actor, id, CanvasAccess::Edit).await?;
        if let Some(document) = &patch.document {
            self.check_document(document)?;
        }

        let change = CanvasChange {
            title: patch.title.as_deref().map(CanvasTitle::parse).transpose()?,
            chain_id: patch.chain_id,
            document: patch.document,
        };
        let summary = self
            .canvases
            .update(id, &change, expected_version, actor.id)
            .await?;

        Ok(VisibleCanvas { summary, ..canvas })
    }

    pub async fn delete(&self, actor: &User, id: CanvasId) -> Result<(), AccountError> {
        self.reach(actor, id, CanvasAccess::Manage).await?;
        Ok(self.canvases.delete(id).await?)
    }

    pub async fn shares(
        &self,
        actor: &User,
        id: CanvasId,
    ) -> Result<Vec<CanvasShare>, AccountError> {
        self.reach(actor, id, CanvasAccess::Manage).await?;
        Ok(self.canvases.shares(id).await?)
    }

    pub async fn share(
        &self,
        actor: &User,
        id: CanvasId,
        email: &str,
        access: ShareAccess,
    ) -> Result<CanvasShare, AccountError> {
        let canvas = self.reach(actor, id, CanvasAccess::Manage).await?;
        let user = self
            .users
            .by_email(&Email::parse(email)?)
            .await?
            .ok_or(AccountError::NotFound("user with this email"))?;

        if canvas.summary.owner == Owner::User(user.id) {
            return Err(AccountError::invalid("the owner already has full access"));
        }

        self.canvases.put_share(id, user.id, access).await?;
        Ok(CanvasShare {
            user,
            access,
            created_at: OffsetDateTime::now_utc(),
        })
    }

    pub async fn unshare(
        &self,
        actor: &User,
        id: CanvasId,
        user: UserId,
    ) -> Result<(), AccountError> {
        if user != actor.id {
            self.reach(actor, id, CanvasAccess::Manage).await?;
        }
        if !self.canvases.remove_share(id, user).await? {
            return Err(AccountError::NotFound("share"));
        }
        Ok(())
    }

    pub async fn links(&self, actor: &User, id: CanvasId) -> Result<Vec<PublicLink>, AccountError> {
        self.reach(actor, id, CanvasAccess::Manage).await?;
        Ok(self.canvases.links(id).await?)
    }

    pub async fn create_link(
        &self,
        actor: &User,
        id: CanvasId,
    ) -> Result<(PublicLink, String), AccountError> {
        self.reach(actor, id, CanvasAccess::Manage).await?;

        let token = self.tokens.mint();
        let link = PublicLink {
            id: LinkId::new(),
            canvas: id,
            created_by: actor.id,
            created_at: OffsetDateTime::now_utc(),
            revoked_at: None,
        };
        self.canvases
            .insert_link(&link, &self.tokens.digest(&token))
            .await?;
        Ok((link, token))
    }

    pub async fn revoke_link(
        &self,
        actor: &User,
        id: CanvasId,
        link: LinkId,
    ) -> Result<(), AccountError> {
        self.reach(actor, id, CanvasAccess::Manage).await?;
        if !self
            .canvases
            .revoke_link(id, link, OffsetDateTime::now_utc())
            .await?
        {
            return Err(AccountError::NotFound("link"));
        }
        Ok(())
    }

    pub async fn open_public(&self, token: &str) -> Result<Canvas, AccountError> {
        if token.is_empty() {
            return Err(AccountError::NotFound("canvas"));
        }
        self.canvases
            .by_link(&self.tokens.digest(token))
            .await?
            .ok_or(AccountError::NotFound("canvas"))
    }

    async fn reach(
        &self,
        actor: &User,
        id: CanvasId,
        needed: CanvasAccess,
    ) -> Result<VisibleCanvas, AccountError> {
        let canvas = self
            .canvases
            .record(id, actor.id)
            .await?
            .and_then(|record| visible(actor.id, record))
            .ok_or(AccountError::NotFound("canvas"))?;

        if canvas.access < needed {
            return Err(AccountError::forbidden(match needed {
                CanvasAccess::View => "you cannot see this canvas",
                CanvasAccess::Edit => "you can only view this canvas",
                CanvasAccess::Manage => "only the canvas owner manages access to it",
            }));
        }
        Ok(canvas)
    }

    fn check_document(&self, document: &serde_json::Value) -> Result<(), AccountError> {
        if !document.is_object() {
            return Err(AccountError::invalid(
                "the canvas document must be a JSON object",
            ));
        }
        let size = serde_json::to_vec(document)
            .map_err(|error| AccountError::invalid(error.to_string()))?
            .len();
        if size > self.max_document_bytes {
            return Err(AccountError::invalid(format!(
                "the canvas takes {size} bytes, at most {} are kept",
                self.max_document_bytes
            )));
        }
        Ok(())
    }
}

fn visible(viewer: UserId, record: CanvasRecord) -> Option<VisibleCanvas> {
    let access = canvas_access(viewer, record.summary.owner, record.team_role, record.share)?;
    Some(VisibleCanvas {
        summary: record.summary,
        team_name: record.team_name,
        access,
    })
}
