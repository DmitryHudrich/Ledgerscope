use std::{collections::BTreeMap, sync::Arc, time::Duration};

use futures::future::join_all;
use reqwest::Client;
use tokio::sync::Semaphore;

use crate::eth::{
    chainlist::Chainlist,
    rpc::pool::{RpcPool, RpcPoolConfig},
};

const PROBES_IN_FLIGHT: usize = 64;

pub struct FleetChain {
    pub pool: RpcPoolConfig,
    pub eager: bool,
}

pub struct RpcFleet {
    chainlist: Option<Arc<Chainlist>>,
    pools: BTreeMap<u64, Arc<RpcPool>>,
    probes: Semaphore,
}

impl RpcFleet {
    pub async fn start(
        http: Client,
        chainlist: Option<Arc<Chainlist>>,
        chains: Vec<FleetChain>,
        rescan_interval: Duration,
    ) -> Arc<Self> {
        let mut eager = Vec::new();
        let mut pools = BTreeMap::new();

        for chain in chains {
            let pool = RpcPool::new(http.clone(), chain.pool, chainlist.clone());
            if chain.eager {
                eager.push(pool.clone());
            }
            pools.insert(pool.chain_id(), pool);
        }

        let fleet = Arc::new(Self {
            chainlist,
            pools,
            probes: Semaphore::new(PROBES_IN_FLIGHT),
        });

        fleet.rescan(&eager).await;

        let lazy: Vec<Arc<RpcPool>> = fleet
            .pools
            .values()
            .filter(|pool| pool.is_empty())
            .cloned()
            .collect();
        let background = fleet.clone();
        tokio::spawn(async move {
            background.rescan(&lazy).await;
            if !rescan_interval.is_zero() {
                background.rescan_forever(rescan_interval).await;
            }
        });

        fleet
    }

    pub fn pool(&self, chain_id: u64) -> Option<Arc<RpcPool>> {
        self.pools.get(&chain_id).cloned()
    }

    pub fn pools(&self) -> impl Iterator<Item = &Arc<RpcPool>> {
        self.pools.values()
    }

    async fn rescan_forever(&self, interval: Duration) {
        loop {
            tokio::time::sleep(interval).await;

            if let Some(chainlist) = &self.chainlist
                && let Err(error) = chainlist.refresh().await
            {
                tracing::warn!("keeping the old chainlist: {error}");
            }

            let pools: Vec<Arc<RpcPool>> = self.pools.values().cloned().collect();
            self.rescan(&pools).await;
        }
    }

    async fn rescan(&self, pools: &[Arc<RpcPool>]) {
        join_all(pools.iter().map(|pool| async move {
            let had_nodes = !pool.is_empty();
            if let Err(error) = pool.rescan(&self.probes).await {
                if had_nodes {
                    tracing::warn!("rpc pool rescan failed, keeping the old nodes: {error}");
                } else {
                    tracing::debug!("rpc pool is still empty: {error}");
                }
            }
        }))
        .await;
    }
}
