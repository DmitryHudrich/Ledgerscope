use std::sync::Arc;

use time::OffsetDateTime;

use domain::account::{Email, TeamId, TeamName, TeamRole, UserId};

use crate::account::{
    AccountError,
    model::{Member, Membership, Team, User},
    ports::{TeamStore, UserStore},
};

pub struct TeamService {
    teams: Arc<dyn TeamStore>,
    users: Arc<dyn UserStore>,
}

impl TeamService {
    pub fn new(teams: Arc<dyn TeamStore>, users: Arc<dyn UserStore>) -> Self {
        Self { teams, users }
    }

    pub async fn create(&self, actor: &User, name: &str) -> Result<Membership, AccountError> {
        let team = Team {
            id: TeamId::new(),
            name: TeamName::parse(name)?,
            created_at: OffsetDateTime::now_utc(),
        };
        self.teams.insert(&team, actor.id).await?;

        Ok(Membership {
            team,
            role: TeamRole::Owner,
            members: 1,
        })
    }

    pub async fn list(&self, actor: &User) -> Result<Vec<Membership>, AccountError> {
        Ok(self.teams.memberships(actor.id).await?)
    }

    pub async fn membership(&self, actor: &User, team: TeamId) -> Result<Membership, AccountError> {
        self.teams
            .membership(team, actor.id)
            .await?
            .ok_or(AccountError::NotFound("team"))
    }

    pub async fn members(&self, actor: &User, team: TeamId) -> Result<Vec<Member>, AccountError> {
        self.membership(actor, team).await?;
        Ok(self.teams.members(team).await?)
    }

    pub async fn add_member(
        &self,
        actor: &User,
        team: TeamId,
        email: &str,
        role: TeamRole,
    ) -> Result<Member, AccountError> {
        let mine = self.membership(actor, team).await?;
        if !mine.role.can_manage() || !mine.role.can_grant(role) {
            return Err(AccountError::forbidden(format!(
                "a team {} cannot add a {role}",
                mine.role
            )));
        }

        let email = Email::parse(email)?;
        let user = self
            .users
            .by_email(&email)
            .await?
            .ok_or(AccountError::NotFound("user with this email"))?;
        self.teams.add_member(team, user.id, role).await?;

        Ok(Member {
            user,
            role,
            joined_at: OffsetDateTime::now_utc(),
        })
    }

    pub async fn change_role(
        &self,
        actor: &User,
        team: TeamId,
        user: UserId,
        role: TeamRole,
    ) -> Result<(), AccountError> {
        let mine = self.membership(actor, team).await?;
        let members = self.teams.members(team).await?;
        let target = members
            .iter()
            .find(|member| member.user.id == user)
            .ok_or(AccountError::NotFound("team member"))?;

        if !mine.role.can_manage()
            || !mine.role.can_grant(role)
            || !mine.role.can_grant(target.role)
        {
            return Err(AccountError::forbidden(format!(
                "a team {} cannot turn a {} into a {role}",
                mine.role, target.role
            )));
        }

        if target.role == TeamRole::Owner && role != TeamRole::Owner && owners(&members) == 1 {
            return Err(AccountError::Conflict(
                "the team needs at least one owner, promote someone else first".to_owned(),
            ));
        }

        Ok(self.teams.set_role(team, user, role).await?)
    }

    pub async fn remove_member(
        &self,
        actor: &User,
        team: TeamId,
        user: UserId,
    ) -> Result<(), AccountError> {
        let mine = self.membership(actor, team).await?;
        let members = self.teams.members(team).await?;
        let target = members
            .iter()
            .find(|member| member.user.id == user)
            .ok_or(AccountError::NotFound("team member"))?;

        let leaving = user == actor.id;
        if !leaving && !(mine.role.can_manage() && mine.role.can_grant(target.role)) {
            return Err(AccountError::forbidden(format!(
                "a team {} cannot remove a {}",
                mine.role, target.role
            )));
        }

        if target.role == TeamRole::Owner && owners(&members) == 1 {
            return Err(AccountError::Conflict(
                "the last owner cannot leave, hand ownership over or delete the team".to_owned(),
            ));
        }

        Ok(self.teams.remove_member(team, user).await?)
    }

    pub async fn rename(
        &self,
        actor: &User,
        team: TeamId,
        name: &str,
    ) -> Result<Team, AccountError> {
        let mine = self.membership(actor, team).await?;
        if !mine.role.can_manage() {
            return Err(AccountError::forbidden(
                "only owners and admins rename a team",
            ));
        }

        let name = TeamName::parse(name)?;
        self.teams.rename(team, &name).await?;
        Ok(Team { name, ..mine.team })
    }

    pub async fn delete(&self, actor: &User, team: TeamId) -> Result<(), AccountError> {
        let mine = self.membership(actor, team).await?;
        if mine.role != TeamRole::Owner {
            return Err(AccountError::forbidden("only an owner deletes a team"));
        }

        Ok(self.teams.delete(team).await?)
    }
}

fn owners(members: &[Member]) -> usize {
    members
        .iter()
        .filter(|member| member.role == TeamRole::Owner)
        .count()
}
