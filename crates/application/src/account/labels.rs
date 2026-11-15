use std::{collections::HashMap, sync::Arc};

use time::OffsetDateTime;

use domain::{
    account::{LabelAccess, LabelId, LabelNote, LabelValue, Owner},
    eth::{AddressLabel, EthAddress},
};

use crate::account::{
    AccountError,
    model::{User, UserLabel},
    ports::{LabelStore, TeamStore},
};

pub const PERSONAL_SOURCE: &str = "you";
pub const TEAM_SOURCE_PREFIX: &str = "team:";

#[derive(Clone, Debug)]
pub struct NewLabel {
    pub owner: Owner,
    pub chain_id: u64,
    pub address: EthAddress,
    pub value: String,
    pub note: String,
}

#[derive(Clone, Debug)]
pub struct VisibleLabel {
    pub label: UserLabel,
    pub access: LabelAccess,
}

pub struct LabelService {
    labels: Arc<dyn LabelStore>,
    teams: Arc<dyn TeamStore>,
}

impl LabelService {
    pub fn new(labels: Arc<dyn LabelStore>, teams: Arc<dyn TeamStore>) -> Self {
        Self { labels, teams }
    }

    pub async fn visible(
        &self,
        actor: &User,
        chain_id: Option<u64>,
        addresses: Option<&[EthAddress]>,
    ) -> Result<Vec<VisibleLabel>, AccountError> {
        Ok(self
            .labels
            .visible(actor.id, chain_id, addresses)
            .await?
            .into_iter()
            .filter_map(|label| {
                let access = label.access(actor.id)?;
                Some(VisibleLabel { label, access })
            })
            .collect())
    }

    pub async fn for_graph(
        &self,
        actor: &User,
        chain_id: u64,
        addresses: &[EthAddress],
    ) -> Result<HashMap<EthAddress, Vec<AddressLabel>>, AccountError> {
        let mut found: HashMap<EthAddress, Vec<AddressLabel>> = HashMap::new();
        let mut visible = self.visible(actor, Some(chain_id), Some(addresses)).await?;
        visible.sort_by_key(|visible| {
            (
                matches!(visible.label.owner, Owner::Team(_)),
                visible.label.created_at,
            )
        });

        for VisibleLabel { label, .. } in visible {
            let source = match &label.team_name {
                Some(team) => format!("{TEAM_SOURCE_PREFIX}{team}"),
                None => PERSONAL_SOURCE.to_owned(),
            };
            found
                .entry(label.address)
                .or_default()
                .push(AddressLabel::new(label.value.into_inner(), source));
        }
        Ok(found)
    }

    pub async fn create(&self, actor: &User, new: NewLabel) -> Result<VisibleLabel, AccountError> {
        let value = LabelValue::parse(&new.value)?;
        let note = LabelNote::parse(&new.note)?.into_inner();

        let (team_name, team_role) = match new.owner {
            Owner::User(user) if user == actor.id => (None, None),
            Owner::User(_) => {
                return Err(AccountError::forbidden("personal labels are yours alone"));
            }
            Owner::Team(team) => {
                let membership = self
                    .teams
                    .membership(team, actor.id)
                    .await?
                    .ok_or(AccountError::NotFound("team"))?;
                if !membership.role.can_edit() {
                    return Err(AccountError::forbidden("team viewers cannot add labels"));
                }
                (
                    Some(membership.team.name.into_inner()),
                    Some(membership.role),
                )
            }
        };

        let now = OffsetDateTime::now_utc();
        let label = UserLabel {
            id: LabelId::new(),
            owner: new.owner,
            team_name,
            team_role,
            chain_id: new.chain_id,
            address: new.address,
            value,
            note,
            created_by: actor.id,
            created_at: now,
            updated_at: now,
        };
        self.labels.insert(&label).await?;

        Ok(VisibleLabel {
            label,
            access: LabelAccess::Write,
        })
    }

    pub async fn update(
        &self,
        actor: &User,
        id: LabelId,
        value: &str,
        note: &str,
    ) -> Result<VisibleLabel, AccountError> {
        let label = self.writable(actor, id).await?;
        let value = LabelValue::parse(value)?;
        let note = LabelNote::parse(note)?.into_inner();
        let now = OffsetDateTime::now_utc();
        self.labels.update(id, &value, &note, now).await?;

        Ok(VisibleLabel {
            label: UserLabel {
                value,
                note,
                updated_at: now,
                ..label
            },
            access: LabelAccess::Write,
        })
    }

    pub async fn delete(&self, actor: &User, id: LabelId) -> Result<(), AccountError> {
        self.writable(actor, id).await?;
        Ok(self.labels.delete(id).await?)
    }

    async fn writable(&self, actor: &User, id: LabelId) -> Result<UserLabel, AccountError> {
        let label = self
            .labels
            .label(id, actor.id)
            .await?
            .ok_or(AccountError::NotFound("label"))?;
        match label.access(actor.id) {
            Some(LabelAccess::Write) => Ok(label),
            Some(LabelAccess::Read) => {
                Err(AccountError::forbidden("team viewers cannot change labels"))
            }
            None => Err(AccountError::NotFound("label")),
        }
    }
}
