use std::sync::Arc;

use time::OffsetDateTime;

use domain::account::{
    CanvasAccess, CanvasId, CanvasOpening, CanvasTitle, CanvasVersionId, Email, LinkId, Owner,
    ShareAccess, UserId, VersionKind, canvas_access,
};

use crate::account::{
    AccountError,
    model::{
        Canvas, CanvasRecord, CanvasShare, CanvasSummary, CanvasVersion, Opened, PublicLink, User,
        VisibleCanvas,
    },
    ports::{CanvasChange, CanvasStore, TeamStore, TokenMint, UserStore},
};

pub const KEPT_AUTOSAVES: usize = 20;
const LABEL_LIMIT: usize = 120;
const CREATED_LABEL: &str = "Created";

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
    pub opening: Option<CanvasOpening>,
    pub snapshot_label: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpenAt {
    #[default]
    Preferred,
    Latest,
    Snapshot,
    Version(CanvasVersionId),
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
            opening: CanvasOpening::default(),
            created_by: actor.id,
            updated_by: actor.id,
            created_at: now,
            updated_at: now,
        };
        self.canvases
            .insert(&Canvas {
                summary: summary.clone(),
                document: new.document.clone(),
            })
            .await?;
        self.record_version(
            actor,
            &summary,
            VersionKind::Snapshot,
            CREATED_LABEL.to_owned(),
            &new.document,
        )
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
        at: OpenAt,
    ) -> Result<(VisibleCanvas, serde_json::Value, Opened), AccountError> {
        let canvas = self.reach(actor, id, CanvasAccess::View).await?;
        let wanted = match at {
            OpenAt::Preferred => match canvas.summary.opening {
                CanvasOpening::Latest => OpenAt::Latest,
                CanvasOpening::Snapshot => OpenAt::Snapshot,
            },
            other => other,
        };

        let picked = match wanted {
            OpenAt::Version(version) => Some(
                self.canvases
                    .version(id, version)
                    .await?
                    .ok_or(AccountError::NotFound("canvas version"))?,
            ),
            OpenAt::Latest => {
                let saved_at = self
                    .canvases
                    .versions(id)
                    .await?
                    .into_iter()
                    .find(|version| version.kind == VersionKind::Snapshot)
                    .map_or(canvas.summary.updated_at, |version| version.created_at);
                self.canvases
                    .latest_version(id, VersionKind::Autosave)
                    .await?
                    .filter(|(version, _)| version.created_at > saved_at)
            }
            OpenAt::Snapshot | OpenAt::Preferred => None,
        };

        if let Some((version, document)) = picked {
            return Ok((canvas, document, Opened::Version(version)));
        }
        let document = self
            .canvases
            .document(id)
            .await?
            .ok_or(AccountError::NotFound("canvas"))?;
        Ok((canvas, document, Opened::Head))
    }

    pub async fn versions(
        &self,
        actor: &User,
        id: CanvasId,
    ) -> Result<Vec<CanvasVersion>, AccountError> {
        self.reach(actor, id, CanvasAccess::View).await?;
        Ok(self.canvases.versions(id).await?)
    }

    pub async fn autosave(
        &self,
        actor: &User,
        id: CanvasId,
        document: serde_json::Value,
    ) -> Result<CanvasVersion, AccountError> {
        let canvas = self.reach(actor, id, CanvasAccess::Edit).await?;
        self.check_document(&document)?;
        let version = self
            .record_version(
                actor,
                &canvas.summary,
                VersionKind::Autosave,
                String::new(),
                &document,
            )
            .await?;
        self.canvases
            .prune_versions(id, VersionKind::Autosave, KEPT_AUTOSAVES)
            .await?;
        Ok(version)
    }

    pub async fn delete_version(
        &self,
        actor: &User,
        id: CanvasId,
        version: CanvasVersionId,
    ) -> Result<(), AccountError> {
        self.reach(actor, id, CanvasAccess::Edit).await?;
        if !self.canvases.delete_version(id, version).await? {
            return Err(AccountError::NotFound("canvas version"));
        }
        Ok(())
    }

    async fn record_version(
        &self,
        actor: &User,
        summary: &CanvasSummary,
        kind: VersionKind,
        label: String,
        document: &serde_json::Value,
    ) -> Result<CanvasVersion, AccountError> {
        let version = CanvasVersion {
            id: CanvasVersionId::new(),
            canvas: summary.id,
            kind,
            label,
            chain_id: summary.chain_id,
            canvas_version: summary.version,
            created_by: actor.id,
            author: actor.name.as_str().to_owned(),
            created_at: OffsetDateTime::now_utc(),
        };
        self.canvases.insert_version(&version, document).await?;
        Ok(version)
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

        let label = snapshot_label(patch.snapshot_label.as_deref())?;
        let snapshot = patch.document.clone();
        let change = CanvasChange {
            title: patch.title.as_deref().map(CanvasTitle::parse).transpose()?,
            chain_id: patch.chain_id,
            document: patch.document,
            opening: patch.opening,
        };
        let summary = self
            .canvases
            .update(id, &change, expected_version, actor.id)
            .await?;
        if let Some(document) = snapshot {
            self.record_version(actor, &summary, VersionKind::Snapshot, label, &document)
                .await?;
        }

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

fn snapshot_label(raw: Option<&str>) -> Result<String, AccountError> {
    let label = raw.unwrap_or_default().trim();
    if label.chars().count() > LABEL_LIMIT {
        return Err(AccountError::invalid(format!(
            "a snapshot name takes at most {LABEL_LIMIT} characters"
        )));
    }
    Ok(label.to_owned())
}

fn visible(viewer: UserId, record: CanvasRecord) -> Option<VisibleCanvas> {
    let access = canvas_access(viewer, record.summary.owner, record.team_role, record.share)?;
    Some(VisibleCanvas {
        summary: record.summary,
        team_name: record.team_name,
        access,
    })
}
