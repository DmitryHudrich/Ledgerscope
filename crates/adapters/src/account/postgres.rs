use std::{io, str::FromStr, time::Duration};

use application::account::{
    Canvas, CanvasRecord, CanvasShare, CanvasStore, CanvasSummary, Credentials, LabelStore, Member,
    Membership, PublicLink, SessionStore, StoreError, Team, TeamStore, User, UserLabel, UserStore,
    ports::CanvasChange,
};
use domain::{
    account::{
        CanvasId, CanvasTitle, DisplayName, Email, LabelId, LabelValue, LinkId, Owner, ShareAccess,
        TeamId, TeamName, TeamRole, UserId,
    },
    eth::EthAddress,
};
use sqlx::{
    AssertSqlSafe, PgPool, Postgres, Row, Transaction,
    postgres::{PgPoolOptions, PgRow},
};
use time::OffsetDateTime;
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

const CANVAS_COLUMNS: &str = "c.id, c.owner_user_id, c.owner_team_id, c.title, c.chain_id, c.version, \
     c.created_by, c.updated_by, c.created_at, c.updated_at";

const LABEL_SELECT: &str = "select l.id, l.owner_user_id, l.owner_team_id, l.chain_id, l.address, l.value, \
     l.note, l.created_by, l.created_at, l.updated_at, t.name as team_name, m.role as team_role \
     from labels l \
     left join teams t on t.id = l.owner_team_id \
     left join team_members m on m.team_id = l.owner_team_id and m.user_id = $1";

#[derive(Clone)]
pub struct PgAccounts {
    pool: PgPool,
}

impl PgAccounts {
    pub async fn connect(url: &str, max_connections: u32) -> Result<Self, io::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await
            .map_err(failed)?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<(), io::Error> {
        MIGRATOR
            .run(&self.pool)
            .await
            .map_err(|error| io::Error::other(format!("postgres migrations failed: {error}")))
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    async fn begin(&self) -> Result<Transaction<'static, Postgres>, io::Error> {
        self.pool.begin().await.map_err(failed)
    }
}

fn failed(error: sqlx::Error) -> io::Error {
    io::Error::other(format!("postgres: {error}"))
}

fn corrupt(what: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("postgres holds a broken {what}"),
    )
}

fn stored(error: sqlx::Error, conflict: &str) -> StoreError {
    match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            StoreError::Conflict(conflict.to_owned())
        }
        _ => StoreError::Io(failed(error)),
    }
}

fn chain_param(chain_id: u64) -> Result<i64, io::Error> {
    i64::try_from(chain_id)
        .map_err(|_| io::Error::other(format!("chain {chain_id} does not fit postgres")))
}

fn chain_of(row: &PgRow, column: &str) -> Result<u64, io::Error> {
    let raw: i64 = row.try_get(column).map_err(failed)?;
    u64::try_from(raw).map_err(|_| corrupt("chain id"))
}

fn address_bytes(address: &EthAddress) -> Vec<u8> {
    address.address().as_slice().to_vec()
}

fn owner_columns(owner: Owner) -> (Option<Uuid>, Option<Uuid>) {
    match owner {
        Owner::User(user) => (Some(user.uuid()), None),
        Owner::Team(team) => (None, Some(team.uuid())),
    }
}

fn owner_of(row: &PgRow) -> Result<Owner, io::Error> {
    let user: Option<Uuid> = row.try_get("owner_user_id").map_err(failed)?;
    let team: Option<Uuid> = row.try_get("owner_team_id").map_err(failed)?;
    match (user, team) {
        (Some(user), None) => Ok(Owner::User(UserId::from_uuid(user))),
        (None, Some(team)) => Ok(Owner::Team(TeamId::from_uuid(team))),
        _ => Err(corrupt("owner")),
    }
}

fn user_of(row: &PgRow, prefix: &str) -> Result<User, io::Error> {
    let column = |name: &str| format!("{prefix}{name}");
    let email: String = row.try_get(column("email").as_str()).map_err(failed)?;
    let name: String = row.try_get(column("name").as_str()).map_err(failed)?;
    Ok(User {
        id: UserId::from_uuid(row.try_get(column("id").as_str()).map_err(failed)?),
        email: Email::parse(&email).map_err(|_| corrupt("email"))?,
        name: DisplayName::parse(&name).map_err(|_| corrupt("user name"))?,
        created_at: row.try_get(column("created_at").as_str()).map_err(failed)?,
    })
}

fn role_of(raw: Option<String>) -> Result<Option<TeamRole>, io::Error> {
    raw.map(|raw| TeamRole::from_str(&raw).map_err(corrupt))
        .transpose()
}

fn share_of(raw: Option<String>) -> Result<Option<ShareAccess>, io::Error> {
    raw.map(|raw| ShareAccess::from_str(&raw).map_err(corrupt))
        .transpose()
}

fn team_of(row: &PgRow) -> Result<Team, io::Error> {
    let name: String = row.try_get("team_name").map_err(failed)?;
    Ok(Team {
        id: TeamId::from_uuid(row.try_get("team_id").map_err(failed)?),
        name: TeamName::parse(&name).map_err(|_| corrupt("team name"))?,
        created_at: row.try_get("team_created_at").map_err(failed)?,
    })
}

fn membership_of(row: &PgRow) -> Result<Membership, io::Error> {
    let role: String = row.try_get("role").map_err(failed)?;
    let members: i64 = row.try_get("members").map_err(failed)?;
    Ok(Membership {
        team: team_of(row)?,
        role: TeamRole::from_str(&role).map_err(corrupt)?,
        members: usize::try_from(members).unwrap_or_default(),
    })
}

fn summary_of(row: &PgRow) -> Result<CanvasSummary, io::Error> {
    let title: String = row.try_get("title").map_err(failed)?;
    Ok(CanvasSummary {
        id: CanvasId::from_uuid(row.try_get("id").map_err(failed)?),
        owner: owner_of(row)?,
        title: CanvasTitle::parse(&title).map_err(|_| corrupt("canvas title"))?,
        chain_id: chain_of(row, "chain_id")?,
        version: row.try_get("version").map_err(failed)?,
        created_by: UserId::from_uuid(row.try_get("created_by").map_err(failed)?),
        updated_by: UserId::from_uuid(row.try_get("updated_by").map_err(failed)?),
        created_at: row.try_get("created_at").map_err(failed)?,
        updated_at: row.try_get("updated_at").map_err(failed)?,
    })
}

fn record_of(row: &PgRow) -> Result<CanvasRecord, io::Error> {
    Ok(CanvasRecord {
        summary: summary_of(row)?,
        team_name: row.try_get("team_name").map_err(failed)?,
        team_role: role_of(row.try_get("team_role").map_err(failed)?)?,
        share: share_of(row.try_get("share").map_err(failed)?)?,
    })
}

fn link_of(row: &PgRow) -> Result<PublicLink, io::Error> {
    Ok(PublicLink {
        id: LinkId::from_uuid(row.try_get("id").map_err(failed)?),
        canvas: CanvasId::from_uuid(row.try_get("canvas_id").map_err(failed)?),
        created_by: UserId::from_uuid(row.try_get("created_by").map_err(failed)?),
        created_at: row.try_get("created_at").map_err(failed)?,
        revoked_at: row.try_get("revoked_at").map_err(failed)?,
    })
}

fn label_of(row: &PgRow) -> Result<UserLabel, io::Error> {
    let address: Vec<u8> = row.try_get("address").map_err(failed)?;
    let value: String = row.try_get("value").map_err(failed)?;
    Ok(UserLabel {
        id: LabelId::from_uuid(row.try_get("id").map_err(failed)?),
        owner: owner_of(row)?,
        team_name: row.try_get("team_name").map_err(failed)?,
        team_role: role_of(row.try_get("team_role").map_err(failed)?)?,
        chain_id: chain_of(row, "chain_id")?,
        address: EthAddress::try_from(address).map_err(|_| corrupt("label address"))?,
        value: LabelValue::parse(&value).map_err(|_| corrupt("label"))?,
        note: row.try_get("note").map_err(failed)?,
        created_by: UserId::from_uuid(row.try_get("created_by").map_err(failed)?),
        created_at: row.try_get("created_at").map_err(failed)?,
        updated_at: row.try_get("updated_at").map_err(failed)?,
    })
}

async fn lock_team(
    tx: &mut Transaction<'static, Postgres>,
    team: TeamId,
) -> Result<(), StoreError> {
    let found = sqlx::query("select id from teams where id = $1 for update")
        .bind(team.uuid())
        .fetch_optional(&mut **tx)
        .await
        .map_err(failed)?;
    if found.is_none() {
        return Err(StoreError::Conflict("the team is gone".to_owned()));
    }
    Ok(())
}

async fn keep_an_owner(
    tx: &mut Transaction<'static, Postgres>,
    team: TeamId,
) -> Result<(), StoreError> {
    let owners: i64 = sqlx::query_scalar(
        "select count(*) from team_members where team_id = $1 and role = 'owner'",
    )
    .bind(team.uuid())
    .fetch_one(&mut **tx)
    .await
    .map_err(failed)?;
    if owners == 0 {
        return Err(StoreError::Conflict(
            "the team needs at least one owner".to_owned(),
        ));
    }
    Ok(())
}

#[async_trait::async_trait]
impl UserStore for PgAccounts {
    async fn insert(&self, user: &User, password_hash: &str) -> Result<(), StoreError> {
        sqlx::query(
            "insert into users (id, email, name, password_hash, created_at) values ($1, $2, $3, $4, $5)",
        )
        .bind(user.id.uuid())
        .bind(user.email.as_str())
        .bind(user.name.as_str())
        .bind(password_hash)
        .bind(user.created_at)
        .execute(&self.pool)
        .await
        .map_err(|error| stored(error, "an account with this email already exists"))?;
        Ok(())
    }

    async fn credentials(&self, email: &Email) -> Result<Option<Credentials>, io::Error> {
        let row = sqlx::query(
            "select id, email, name, created_at, password_hash from users where email = $1",
        )
        .bind(email.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(failed)?;
        row.map(|row| {
            Ok(Credentials {
                user: user_of(&row, "")?,
                password_hash: row.try_get("password_hash").map_err(failed)?,
            })
        })
        .transpose()
    }

    async fn credentials_of(&self, id: UserId) -> Result<Option<Credentials>, io::Error> {
        let row = sqlx::query(
            "select id, email, name, created_at, password_hash from users where id = $1",
        )
        .bind(id.uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(failed)?;
        row.map(|row| {
            Ok(Credentials {
                user: user_of(&row, "")?,
                password_hash: row.try_get("password_hash").map_err(failed)?,
            })
        })
        .transpose()
    }

    async fn by_email(&self, email: &Email) -> Result<Option<User>, io::Error> {
        sqlx::query("select id, email, name, created_at from users where email = $1")
            .bind(email.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(failed)?
            .map(|row| user_of(&row, ""))
            .transpose()
    }

    async fn rename(&self, id: UserId, name: &DisplayName) -> Result<(), io::Error> {
        sqlx::query("update users set name = $2, updated_at = now() where id = $1")
            .bind(id.uuid())
            .bind(name.as_str())
            .execute(&self.pool)
            .await
            .map_err(failed)?;
        Ok(())
    }

    async fn set_password_hash(&self, id: UserId, password_hash: &str) -> Result<(), io::Error> {
        sqlx::query("update users set password_hash = $2, updated_at = now() where id = $1")
            .bind(id.uuid())
            .bind(password_hash)
            .execute(&self.pool)
            .await
            .map_err(failed)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl SessionStore for PgAccounts {
    async fn insert(
        &self,
        digest: &[u8],
        user: UserId,
        expires_at: OffsetDateTime,
    ) -> Result<(), io::Error> {
        sqlx::query("insert into sessions (digest, user_id, expires_at) values ($1, $2, $3)")
            .bind(digest)
            .bind(user.uuid())
            .bind(expires_at)
            .execute(&self.pool)
            .await
            .map_err(failed)?;
        Ok(())
    }

    async fn user(&self, digest: &[u8], now: OffsetDateTime) -> Result<Option<User>, io::Error> {
        sqlx::query(
            "select u.id, u.email, u.name, u.created_at from sessions s \
             join users u on u.id = s.user_id where s.digest = $1 and s.expires_at > $2",
        )
        .bind(digest)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(failed)?
        .map(|row| user_of(&row, ""))
        .transpose()
    }

    async fn delete(&self, digest: &[u8]) -> Result<(), io::Error> {
        sqlx::query("delete from sessions where digest = $1")
            .bind(digest)
            .execute(&self.pool)
            .await
            .map_err(failed)?;
        Ok(())
    }

    async fn delete_all(&self, user: UserId) -> Result<(), io::Error> {
        sqlx::query("delete from sessions where user_id = $1")
            .bind(user.uuid())
            .execute(&self.pool)
            .await
            .map_err(failed)?;
        Ok(())
    }

    async fn purge_expired(&self, now: OffsetDateTime) -> Result<u64, io::Error> {
        Ok(sqlx::query("delete from sessions where expires_at <= $1")
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(failed)?
            .rows_affected())
    }
}

const MEMBERSHIP_SELECT: &str = "select t.id as team_id, t.name as team_name, t.created_at as team_created_at, m.role, \
     (select count(*) from team_members x where x.team_id = t.id) as members \
     from team_members m join teams t on t.id = m.team_id";

#[async_trait::async_trait]
impl TeamStore for PgAccounts {
    async fn insert(&self, team: &Team, owner: UserId) -> Result<(), StoreError> {
        let mut tx = self.begin().await?;
        sqlx::query("insert into teams (id, name, created_at) values ($1, $2, $3)")
            .bind(team.id.uuid())
            .bind(team.name.as_str())
            .bind(team.created_at)
            .execute(&mut *tx)
            .await
            .map_err(failed)?;
        sqlx::query("insert into team_members (team_id, user_id, role, joined_at) values ($1, $2, 'owner', $3)")
            .bind(team.id.uuid())
            .bind(owner.uuid())
            .bind(team.created_at)
            .execute(&mut *tx)
            .await
            .map_err(failed)?;
        tx.commit().await.map_err(failed)?;
        Ok(())
    }

    async fn memberships(&self, user: UserId) -> Result<Vec<Membership>, io::Error> {
        sqlx::query(AssertSqlSafe(format!(
            "{MEMBERSHIP_SELECT} where m.user_id = $1 order by lower(t.name), t.id"
        )))
        .bind(user.uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(failed)?
        .iter()
        .map(membership_of)
        .collect()
    }

    async fn membership(
        &self,
        team: TeamId,
        user: UserId,
    ) -> Result<Option<Membership>, io::Error> {
        sqlx::query(AssertSqlSafe(format!(
            "{MEMBERSHIP_SELECT} where m.user_id = $1 and m.team_id = $2"
        )))
        .bind(user.uuid())
        .bind(team.uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(failed)?
        .map(|row| membership_of(&row))
        .transpose()
    }

    async fn members(&self, team: TeamId) -> Result<Vec<Member>, io::Error> {
        sqlx::query(
            "select u.id, u.email, u.name, u.created_at, m.role, m.joined_at \
             from team_members m join users u on u.id = m.user_id where m.team_id = $1 \
             order by case m.role when 'owner' then 0 when 'admin' then 1 when 'editor' then 2 else 3 end, lower(u.name)",
        )
        .bind(team.uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(failed)?
        .iter()
        .map(|row| {
            let role: String = row.try_get("role").map_err(failed)?;
            Ok(Member {
                user: user_of(row, "")?,
                role: TeamRole::from_str(&role).map_err(corrupt)?,
                joined_at: row.try_get("joined_at").map_err(failed)?,
            })
        })
        .collect()
    }

    async fn add_member(
        &self,
        team: TeamId,
        user: UserId,
        role: TeamRole,
    ) -> Result<(), StoreError> {
        sqlx::query("insert into team_members (team_id, user_id, role) values ($1, $2, $3)")
            .bind(team.uuid())
            .bind(user.uuid())
            .bind(role.as_str())
            .execute(&self.pool)
            .await
            .map_err(|error| stored(error, "this user is already in the team"))?;
        Ok(())
    }

    async fn set_role(&self, team: TeamId, user: UserId, role: TeamRole) -> Result<(), StoreError> {
        let mut tx = self.begin().await?;
        lock_team(&mut tx, team).await?;
        let changed =
            sqlx::query("update team_members set role = $3 where team_id = $1 and user_id = $2")
                .bind(team.uuid())
                .bind(user.uuid())
                .bind(role.as_str())
                .execute(&mut *tx)
                .await
                .map_err(failed)?
                .rows_affected();
        if changed == 0 {
            return Err(StoreError::Conflict(
                "this user is not in the team".to_owned(),
            ));
        }
        keep_an_owner(&mut tx, team).await?;
        tx.commit().await.map_err(failed)?;
        Ok(())
    }

    async fn remove_member(&self, team: TeamId, user: UserId) -> Result<(), StoreError> {
        let mut tx = self.begin().await?;
        lock_team(&mut tx, team).await?;
        let removed = sqlx::query("delete from team_members where team_id = $1 and user_id = $2")
            .bind(team.uuid())
            .bind(user.uuid())
            .execute(&mut *tx)
            .await
            .map_err(failed)?
            .rows_affected();
        if removed == 0 {
            return Err(StoreError::Conflict(
                "this user is not in the team".to_owned(),
            ));
        }
        keep_an_owner(&mut tx, team).await?;
        tx.commit().await.map_err(failed)?;
        Ok(())
    }

    async fn rename(&self, team: TeamId, name: &TeamName) -> Result<(), io::Error> {
        sqlx::query("update teams set name = $2 where id = $1")
            .bind(team.uuid())
            .bind(name.as_str())
            .execute(&self.pool)
            .await
            .map_err(failed)?;
        Ok(())
    }

    async fn delete(&self, team: TeamId) -> Result<(), io::Error> {
        sqlx::query("delete from teams where id = $1")
            .bind(team.uuid())
            .execute(&self.pool)
            .await
            .map_err(failed)?;
        Ok(())
    }
}

fn canvas_select(filter: &str) -> String {
    format!(
        "select {CANVAS_COLUMNS}, t.name as team_name, m.role as team_role, s.access as share \
         from canvases c \
         left join teams t on t.id = c.owner_team_id \
         left join team_members m on m.team_id = c.owner_team_id and m.user_id = $1 \
         left join canvas_shares s on s.canvas_id = c.id and s.user_id = $1 \
         where {filter}"
    )
}

#[async_trait::async_trait]
impl CanvasStore for PgAccounts {
    async fn insert(&self, canvas: &Canvas) -> Result<(), StoreError> {
        let summary = &canvas.summary;
        let (owner_user, owner_team) = owner_columns(summary.owner);
        sqlx::query(
            "insert into canvases (id, owner_user_id, owner_team_id, title, chain_id, document, version, \
             created_by, updated_by, created_at, updated_at) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(summary.id.uuid())
        .bind(owner_user)
        .bind(owner_team)
        .bind(summary.title.as_str())
        .bind(chain_param(summary.chain_id)?)
        .bind(sqlx::types::Json(&canvas.document))
        .bind(summary.version)
        .bind(summary.created_by.uuid())
        .bind(summary.updated_by.uuid())
        .bind(summary.created_at)
        .bind(summary.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|error| stored(error, "this canvas already exists"))?;
        Ok(())
    }

    async fn record(
        &self,
        id: CanvasId,
        viewer: UserId,
    ) -> Result<Option<CanvasRecord>, io::Error> {
        sqlx::query(AssertSqlSafe(canvas_select("c.id = $2")))
            .bind(viewer.uuid())
            .bind(id.uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(failed)?
            .map(|row| record_of(&row))
            .transpose()
    }

    async fn document(&self, id: CanvasId) -> Result<Option<serde_json::Value>, io::Error> {
        let row = sqlx::query("select document from canvases where id = $1")
            .bind(id.uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(failed)?;
        row.map(|row| {
            row.try_get::<sqlx::types::Json<serde_json::Value>, _>("document")
                .map(|document| document.0)
                .map_err(failed)
        })
        .transpose()
    }

    async fn list(&self, viewer: UserId) -> Result<Vec<CanvasRecord>, io::Error> {
        sqlx::query(AssertSqlSafe(format!(
            "{} order by c.updated_at desc",
            canvas_select("c.owner_user_id = $1 or m.user_id is not null or s.user_id is not null")
        )))
        .bind(viewer.uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(failed)?
        .iter()
        .map(record_of)
        .collect()
    }

    async fn update(
        &self,
        id: CanvasId,
        change: &CanvasChange,
        expected_version: i64,
        by: UserId,
    ) -> Result<CanvasSummary, StoreError> {
        let chain = change.chain_id.map(chain_param).transpose()?;
        let row = sqlx::query(AssertSqlSafe(format!(
            "update canvases c set title = coalesce($3, c.title), chain_id = coalesce($4, c.chain_id), \
             document = coalesce($5, c.document), version = c.version + 1, updated_by = $6, updated_at = now() \
             where c.id = $1 and c.version = $2 returning {CANVAS_COLUMNS}"
        )))
        .bind(id.uuid())
        .bind(expected_version)
        .bind(change.title.as_ref().map(CanvasTitle::as_str))
        .bind(chain)
        .bind(change.document.as_ref().map(sqlx::types::Json))
        .bind(by.uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(failed)?;

        match row {
            Some(row) => Ok(summary_of(&row)?),
            None => {
                let current: Option<i64> =
                    sqlx::query_scalar("select version from canvases where id = $1")
                        .bind(id.uuid())
                        .fetch_optional(&self.pool)
                        .await
                        .map_err(failed)?;
                Err(StoreError::Conflict(match current {
                    Some(version) => format!(
                        "the canvas changed since version {expected_version}, it is at version {version} now"
                    ),
                    None => "the canvas is gone".to_owned(),
                }))
            }
        }
    }

    async fn delete(&self, id: CanvasId) -> Result<(), io::Error> {
        sqlx::query("delete from canvases where id = $1")
            .bind(id.uuid())
            .execute(&self.pool)
            .await
            .map_err(failed)?;
        Ok(())
    }

    async fn shares(&self, id: CanvasId) -> Result<Vec<CanvasShare>, io::Error> {
        sqlx::query(
            "select u.id, u.email, u.name, u.created_at, s.access, s.created_at as shared_at \
             from canvas_shares s join users u on u.id = s.user_id where s.canvas_id = $1 order by s.created_at",
        )
        .bind(id.uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(failed)?
        .iter()
        .map(|row| {
            Ok(CanvasShare {
                user: user_of(row, "")?,
                access: share_of(row.try_get("access").map_err(failed)?)?.ok_or_else(|| corrupt("share"))?,
                created_at: row.try_get("shared_at").map_err(failed)?,
            })
        })
        .collect()
    }

    async fn put_share(
        &self,
        id: CanvasId,
        user: UserId,
        access: ShareAccess,
    ) -> Result<(), io::Error> {
        sqlx::query(
            "insert into canvas_shares (canvas_id, user_id, access) values ($1, $2, $3) \
             on conflict (canvas_id, user_id) do update set access = excluded.access",
        )
        .bind(id.uuid())
        .bind(user.uuid())
        .bind(access.as_str())
        .execute(&self.pool)
        .await
        .map_err(failed)?;
        Ok(())
    }

    async fn remove_share(&self, id: CanvasId, user: UserId) -> Result<bool, io::Error> {
        Ok(
            sqlx::query("delete from canvas_shares where canvas_id = $1 and user_id = $2")
                .bind(id.uuid())
                .bind(user.uuid())
                .execute(&self.pool)
                .await
                .map_err(failed)?
                .rows_affected()
                > 0,
        )
    }

    async fn insert_link(&self, link: &PublicLink, digest: &[u8]) -> Result<(), io::Error> {
        sqlx::query(
            "insert into canvas_links (id, canvas_id, digest, created_by, created_at) values ($1, $2, $3, $4, $5)",
        )
        .bind(link.id.uuid())
        .bind(link.canvas.uuid())
        .bind(digest)
        .bind(link.created_by.uuid())
        .bind(link.created_at)
        .execute(&self.pool)
        .await
        .map_err(failed)?;
        Ok(())
    }

    async fn links(&self, id: CanvasId) -> Result<Vec<PublicLink>, io::Error> {
        sqlx::query(
            "select id, canvas_id, created_by, created_at, revoked_at from canvas_links \
             where canvas_id = $1 order by created_at desc",
        )
        .bind(id.uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(failed)?
        .iter()
        .map(link_of)
        .collect()
    }

    async fn revoke_link(
        &self,
        id: CanvasId,
        link: LinkId,
        now: OffsetDateTime,
    ) -> Result<bool, io::Error> {
        Ok(sqlx::query(
            "update canvas_links set revoked_at = $3 where id = $2 and canvas_id = $1 and revoked_at is null",
        )
        .bind(id.uuid())
        .bind(link.uuid())
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(failed)?
        .rows_affected()
            > 0)
    }

    async fn by_link(&self, digest: &[u8]) -> Result<Option<Canvas>, io::Error> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "select {CANVAS_COLUMNS}, c.document from canvas_links l join canvases c on c.id = l.canvas_id \
             where l.digest = $1 and l.revoked_at is null"
        )))
        .bind(digest)
        .fetch_optional(&self.pool)
        .await
        .map_err(failed)?;
        row.map(|row| {
            Ok(Canvas {
                summary: summary_of(&row)?,
                document: row
                    .try_get::<sqlx::types::Json<serde_json::Value>, _>("document")
                    .map_err(failed)?
                    .0,
            })
        })
        .transpose()
    }
}

#[async_trait::async_trait]
impl LabelStore for PgAccounts {
    async fn visible(
        &self,
        viewer: UserId,
        chain_id: Option<u64>,
        addresses: Option<&[EthAddress]>,
    ) -> Result<Vec<UserLabel>, io::Error> {
        let chain = chain_id.map(chain_param).transpose()?;
        let addresses: Option<Vec<Vec<u8>>> =
            addresses.map(|addresses| addresses.iter().map(address_bytes).collect());
        sqlx::query(AssertSqlSafe(format!(
            "{LABEL_SELECT} where (l.owner_user_id = $1 or m.user_id is not null) \
             and ($2::bigint is null or l.chain_id = $2) \
             and ($3::bytea[] is null or l.address = any($3)) \
             order by l.updated_at desc"
        )))
        .bind(viewer.uuid())
        .bind(chain)
        .bind(addresses)
        .fetch_all(&self.pool)
        .await
        .map_err(failed)?
        .iter()
        .map(label_of)
        .collect()
    }

    async fn label(&self, id: LabelId, viewer: UserId) -> Result<Option<UserLabel>, io::Error> {
        sqlx::query(AssertSqlSafe(format!("{LABEL_SELECT} where l.id = $2")))
            .bind(viewer.uuid())
            .bind(id.uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(failed)?
            .map(|row| label_of(&row))
            .transpose()
    }

    async fn insert(&self, label: &UserLabel) -> Result<(), StoreError> {
        let (owner_user, owner_team) = owner_columns(label.owner);
        sqlx::query(
            "insert into labels (id, owner_user_id, owner_team_id, chain_id, address, value, note, \
             created_by, created_at, updated_at) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(label.id.uuid())
        .bind(owner_user)
        .bind(owner_team)
        .bind(chain_param(label.chain_id)?)
        .bind(address_bytes(&label.address))
        .bind(label.value.as_str())
        .bind(&label.note)
        .bind(label.created_by.uuid())
        .bind(label.created_at)
        .bind(label.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|error| stored(error, "this address already carries that label"))?;
        Ok(())
    }

    async fn update(
        &self,
        id: LabelId,
        value: &LabelValue,
        note: &str,
        now: OffsetDateTime,
    ) -> Result<(), StoreError> {
        sqlx::query("update labels set value = $2, note = $3, updated_at = $4 where id = $1")
            .bind(id.uuid())
            .bind(value.as_str())
            .bind(note)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|error| stored(error, "this address already carries that label"))?;
        Ok(())
    }

    async fn delete(&self, id: LabelId) -> Result<(), io::Error> {
        sqlx::query("delete from labels where id = $1")
            .bind(id.uuid())
            .execute(&self.pool)
            .await
            .map_err(failed)?;
        Ok(())
    }
}
