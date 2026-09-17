use std::sync::Arc;

use anyhow::Context;

use adapters::eth::{ClickhouseTxRepository, RedisTxCache, RpcTxSource};
use application::eth::{
    EthExplorer, ExploreLimits, FetchingTxIndex, StoringEthTxSource,
    classificator::FulliestEthTxClassificator,
    ports::{EthRpcSource, EthTxCache, EthTxIndex, EthTxSource},
};
use reqwest::{Proxy, Url};

use crate::config::{Config, EthRpcConfig, GraphConfig, StorageConfig};

struct Storage {
    source: Arc<dyn EthTxSource>,
    index: Option<Arc<dyn EthTxIndex>>,
}

#[derive(Clone)]
pub struct AppState {
    explorer: Arc<EthExplorer>,
    rpc: Arc<dyn EthRpcSource>,
}

impl AppState {
    pub fn new(explorer: Arc<EthExplorer>, rpc: Arc<dyn EthRpcSource>) -> Self {
        Self { explorer, rpc }
    }

    pub async fn from_config(config: &Config) -> anyhow::Result<Self> {
        let rpc = &config.eth.rpc;
        let http_client = http_client(rpc)?;
        let rpc_url = Url::parse(&rpc.url)
            .with_context(|| format!("eth.rpc.url is not a valid URL: {}", rpc.url))?;

        let rpc_source = Arc::new(RpcTxSource::new(http_client, rpc_url, rpc.rps));
        let storage = storage(&config.storage, rpc_source.clone()).await?;
        let tx_classificator = Arc::new(FulliestEthTxClassificator::new(rpc_source.clone()));

        let index = storage.index.unwrap_or_else(|| {
            Arc::new(FetchingTxIndex::new(storage.source.clone())) as Arc<dyn EthTxIndex>
        });

        Ok(Self::new(
            Arc::new(EthExplorer::new(
                index,
                storage.source,
                tx_classificator,
                limits(&config.graph),
            )),
            rpc_source,
        ))
    }

    pub fn explorer(&self) -> &EthExplorer {
        &self.explorer
    }

    pub async fn head_block(&self) -> Option<u64> {
        match self.rpc.head_block().await {
            Ok(head) => Some(head),
            Err(error) => {
                tracing::warn!("the node did not tell its head block: {error}");
                None
            }
        }
    }
}

fn limits(graph: &GraphConfig) -> ExploreLimits {
    ExploreLimits {
        max_roots: graph.max_roots,
        max_depth: graph.max_depth,
        max_nodes: graph.max_nodes,
        max_edges: graph.max_edges,
        max_blocks: graph.max_blocks,
    }
}

async fn storage(
    storage: &StorageConfig,
    upstream: Arc<dyn EthTxSource>,
) -> anyhow::Result<Storage> {
    if !storage.persist_txs {
        tracing::info!("storage.persist_txs is off, every request goes to the rpc");
        return Ok(Storage {
            source: upstream,
            index: None,
        });
    }

    let clickhouse = &storage.clickhouse;
    let url = Url::parse(&clickhouse.url).with_context(|| {
        format!(
            "storage.clickhouse.url is not a valid URL: {}",
            clickhouse.url
        )
    })?;

    let repository = Arc::new(ClickhouseTxRepository::new(
        reqwest::Client::new(),
        url,
        clickhouse.database.clone(),
        clickhouse.user.clone(),
        clickhouse.password.clone(),
    ));

    repository
        .ensure_schema()
        .await
        .context("failed to prepare the clickhouse schema")?;

    let cache = tx_cache(&storage.redis.url).await;

    Ok(Storage {
        source: Arc::new(StoringEthTxSource::new(upstream, repository.clone(), cache)),
        index: Some(repository as Arc<dyn EthTxIndex>),
    })
}

async fn tx_cache(url: &str) -> Option<Arc<dyn EthTxCache>> {
    if url.is_empty() {
        tracing::info!("storage.redis.url is empty, running without a cache");
        return None;
    }

    match RedisTxCache::connect(url).await {
        Ok(cache) => Some(Arc::new(cache) as Arc<dyn EthTxCache>),
        Err(error) => {
            tracing::warn!("running without a cache, redis is not available: {error}");
            None
        }
    }
}

fn http_client(rpc: &EthRpcConfig) -> anyhow::Result<reqwest::Client> {
    let mut client = reqwest::ClientBuilder::new();

    if !rpc.proxy.is_empty() {
        client =
            client.proxy(Proxy::all(&rpc.proxy).with_context(|| {
                format!("eth.rpc.proxy is not a valid proxy URL: {}", rpc.proxy)
            })?);
    }

    client.build().context("failed to build the HTTP client")
}
