use std::sync::{Arc, OnceLock};

use time::{Duration, OffsetDateTime};

use domain::account::{DisplayName, Email, Password, UserId};

use crate::account::{
    AccountError,
    model::User,
    ports::{PasswordHasher, SessionStore, TokenMint, UserStore},
};

const DUMMY_PASSWORD: &str = "ledgerscope-timing-equaliser";

#[derive(Clone, Debug)]
pub struct Session {
    pub token: String,
    pub user: User,
    pub expires_at: OffsetDateTime,
}

pub struct AuthService {
    users: Arc<dyn UserStore>,
    sessions: Arc<dyn SessionStore>,
    hasher: Arc<dyn PasswordHasher>,
    tokens: Arc<dyn TokenMint>,
    session_ttl: Duration,
    registration_open: bool,
    dummy_hash: OnceLock<String>,
}

impl AuthService {
    pub fn new(
        users: Arc<dyn UserStore>,
        sessions: Arc<dyn SessionStore>,
        hasher: Arc<dyn PasswordHasher>,
        tokens: Arc<dyn TokenMint>,
        session_ttl: Duration,
        registration_open: bool,
    ) -> Self {
        Self {
            users,
            sessions,
            hasher,
            tokens,
            session_ttl,
            registration_open,
            dummy_hash: OnceLock::new(),
        }
    }

    pub fn registration_open(&self) -> bool {
        self.registration_open
    }

    pub fn session_ttl(&self) -> Duration {
        self.session_ttl
    }

    pub async fn register(
        &self,
        email: &str,
        name: &str,
        password: &str,
    ) -> Result<Session, AccountError> {
        if !self.registration_open {
            return Err(AccountError::forbidden(
                "registration is closed on this server",
            ));
        }

        let email = Email::parse(email)?;
        let name = DisplayName::parse(name)?;
        let password = Password::parse(password)?;
        let hash = self.hash(password).await?;

        let user = User {
            id: UserId::new(),
            email,
            name,
            created_at: OffsetDateTime::now_utc(),
        };
        self.users.insert(&user, &hash).await?;

        self.open_session(user).await
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<Session, AccountError> {
        let credentials = match Email::parse(email) {
            Ok(email) => self.users.credentials(&email).await?,
            Err(_) => None,
        };

        let hash = credentials
            .as_ref()
            .map(|credentials| credentials.password_hash.clone())
            .unwrap_or_else(|| self.dummy_hash());
        let matches = self.verify(password.to_owned(), hash).await?;

        match credentials {
            Some(credentials) if matches => self.open_session(credentials.user).await,
            _ => Err(AccountError::invalid("wrong email or password")),
        }
    }

    pub async fn authenticate(&self, token: &str) -> Result<Option<User>, AccountError> {
        if token.is_empty() {
            return Ok(None);
        }
        let digest = self.tokens.digest(token);
        Ok(self
            .sessions
            .user(&digest, OffsetDateTime::now_utc())
            .await?)
    }

    pub async fn logout(&self, token: &str) -> Result<(), AccountError> {
        self.sessions.delete(&self.tokens.digest(token)).await?;
        Ok(())
    }

    pub async fn logout_everywhere(&self, user: &User) -> Result<(), AccountError> {
        self.sessions.delete_all(user.id).await?;
        Ok(())
    }

    pub async fn rename(&self, user: &User, name: &str) -> Result<User, AccountError> {
        let name = DisplayName::parse(name)?;
        self.users.rename(user.id, &name).await?;
        Ok(User {
            name,
            ..user.clone()
        })
    }

    pub async fn change_password(
        &self,
        user: &User,
        current: &str,
        next: &str,
    ) -> Result<Session, AccountError> {
        let next = Password::parse(next)?;
        let credentials = self
            .users
            .credentials_of(user.id)
            .await?
            .ok_or(AccountError::Unauthenticated)?;

        if !self
            .verify(current.to_owned(), credentials.password_hash)
            .await?
        {
            return Err(AccountError::invalid("the current password is wrong"));
        }

        let hash = self.hash(next).await?;
        self.users.set_password_hash(user.id, &hash).await?;
        self.sessions.delete_all(user.id).await?;

        self.open_session(credentials.user).await
    }

    pub async fn purge_expired(&self) -> Result<u64, AccountError> {
        Ok(self
            .sessions
            .purge_expired(OffsetDateTime::now_utc())
            .await?)
    }

    async fn open_session(&self, user: User) -> Result<Session, AccountError> {
        let token = self.tokens.mint();
        let expires_at = OffsetDateTime::now_utc() + self.session_ttl;
        self.sessions
            .insert(&self.tokens.digest(&token), user.id, expires_at)
            .await?;

        Ok(Session {
            token,
            user,
            expires_at,
        })
    }

    async fn hash(&self, password: Password) -> Result<String, AccountError> {
        let hasher = self.hasher.clone();
        tokio::task::spawn_blocking(move || hasher.hash(password.expose()))
            .await
            .map_err(|error| AccountError::Storage(std::io::Error::other(error)))?
            .map_err(AccountError::Storage)
    }

    async fn verify(&self, password: String, hash: String) -> Result<bool, AccountError> {
        let hasher = self.hasher.clone();
        tokio::task::spawn_blocking(move || hasher.verify(&password, &hash))
            .await
            .map_err(|error| AccountError::Storage(std::io::Error::other(error)))
    }

    fn dummy_hash(&self) -> String {
        self.dummy_hash
            .get_or_init(|| self.hasher.hash(DUMMY_PASSWORD).unwrap_or_default())
            .clone()
    }
}
