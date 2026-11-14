use std::{collections::BTreeMap, sync::Arc};

use time::OffsetDateTime;

use domain::account::RpcNodeId;

use crate::account::{
    AccountError,
    model::{RpcNode, User},
    ports::{RpcNodeProbe, RpcNodeStore},
};

const MAX_URL_CHARS: usize = 2048;

pub struct RpcNodeService {
    nodes: Arc<dyn RpcNodeStore>,
    probe: Arc<dyn RpcNodeProbe>,
    max_per_chain: usize,
}

impl RpcNodeService {
    pub fn new(
        nodes: Arc<dyn RpcNodeStore>,
        probe: Arc<dyn RpcNodeProbe>,
        max_per_chain: usize,
    ) -> Self {
        Self {
            nodes,
            probe,
            max_per_chain,
        }
    }

    pub fn max_per_chain(&self) -> usize {
        self.max_per_chain
    }

    pub async fn list(
        &self,
        actor: &User,
        chain_id: Option<u64>,
    ) -> Result<Vec<RpcNode>, AccountError> {
        Ok(self.nodes.list(actor.id, chain_id).await?)
    }

    pub async fn by_chain(&self, actor: &User) -> Result<BTreeMap<u64, Vec<String>>, AccountError> {
        let mut found: BTreeMap<u64, Vec<String>> = BTreeMap::new();
        for node in self.nodes.list(actor.id, None).await? {
            found.entry(node.chain_id).or_default().push(node.url);
        }
        Ok(found)
    }

    pub async fn add(
        &self,
        actor: &User,
        chain_id: u64,
        url: &str,
    ) -> Result<RpcNode, AccountError> {
        let url = url.trim();
        if url.is_empty() {
            return Err(AccountError::invalid("url must not be empty"));
        }
        if url.chars().count() > MAX_URL_CHARS {
            return Err(AccountError::invalid(format!(
                "url must be at most {MAX_URL_CHARS} characters"
            )));
        }

        let held = self.nodes.list(actor.id, Some(chain_id)).await?.len();
        if held >= self.max_per_chain {
            return Err(AccountError::invalid(format!(
                "at most {} own nodes per chain",
                self.max_per_chain
            )));
        }

        let url = self
            .probe
            .probe(chain_id, url)
            .await
            .map_err(AccountError::Invalid)?;

        let node = RpcNode {
            id: RpcNodeId::new(),
            user_id: actor.id,
            chain_id,
            url,
            created_at: OffsetDateTime::now_utc(),
        };
        self.nodes.insert(&node).await?;
        Ok(node)
    }

    pub async fn delete(&self, actor: &User, id: RpcNodeId) -> Result<(), AccountError> {
        if self.nodes.delete(id, actor.id).await? {
            Ok(())
        } else {
            Err(AccountError::NotFound("rpc node"))
        }
    }
}
