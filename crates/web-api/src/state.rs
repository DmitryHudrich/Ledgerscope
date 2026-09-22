use std::{path::Path, sync::Arc};

use anyhow::Context;

use adapters::eth::{ClickhouseTxRepository, RedisTxCache, RpcTxSource};
use application::eth::{
    CachingActorResolver, EthExplorer, ExploreLimits, FetchingTxIndex, JsonLabelProvider,
    StoringEthTxSource,
    classificator::FulliestEthTxClassificator,
    ports::{ActorRepository, EthRpcSource, EthTxCache, EthTxIndex, EthTxSource, LabelProvider},
};
use reqwest::{Proxy, Url};

use crate::config::{Config, GraphConfig, StorageConfig};

struct Storage {
    source: Arc<dyn EthTxSource>,
    index: Option<Arc<dyn EthTxIndex>>,
    actors: Option<Arc<dyn ActorRepository>>,
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
        let proxy_url = config
            .proxy
            .trim()
            .is_empty()
            .then_some(config.proxy.trim().to_string());
        let http_client = get_http_client(proxy_url.clone())?;
        let rpc_url = Url::parse(&rpc.url)
            .with_context(|| format!("eth.rpc.url is not a valid URL: {}", rpc.url))?;

        let rpc_source = Arc::new(RpcTxSource::new(http_client, rpc_url, rpc.rps));
        let storage = storage(&config.storage, rpc_source.clone()).await?;
        let tx_classificator = Arc::new(FulliestEthTxClassificator::new());

        let label_provider = Arc::new(
            JsonLabelProvider::read_all(Path::new(&config.eth.etherscan_json_path), 1)
                .with_context(|| {
                    format!(
                        "failed to read the labels from {}",
                        config.eth.etherscan_json_path
                    )
                })?,
        ) as Arc<dyn LabelProvider>;

        let actor_resolver = Arc::new(CachingActorResolver::new(
            rpc_source.clone(),
            storage.actors,
            label_provider.clone(),
        ));

        let index = storage.index.unwrap_or_else(|| {
            Arc::new(FetchingTxIndex::new(storage.source.clone())) as Arc<dyn EthTxIndex>
        });

        Ok(Self::new(
            Arc::new(EthExplorer::new(
                index,
                storage.source,
                tx_classificator,
                actor_resolver,
                label_provider,
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
            actors: None,
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
        index: Some(repository.clone() as Arc<dyn EthTxIndex>),
        actors: Some(repository as Arc<dyn ActorRepository>),
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

fn get_http_client(proxy_url: Option<String>) -> anyhow::Result<reqwest::Client> {
    let mut client = reqwest::ClientBuilder::new();

    if let Some(proxy_url) = proxy_url {
        client =
            client.proxy(Proxy::all(&proxy_url).with_context(|| {
                format!("eth.rpc.proxy is not a valid proxy URL: {}", proxy_url)
            })?);
    }

    client.build().context("failed to build the HTTP client")
}
