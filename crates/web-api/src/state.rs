use std::sync::Arc;

use anyhow::Context;

use adapters::eth::{ClickhouseTxRepository, RedisTxCache, RpcTxSource};
use application::eth::{
    EthFetcher, StoringEthTxSource,
    classificator::FulliestEthTxClassificator,
    ports::{EthTxCache, EthTxSource},
};
use reqwest::{Proxy, Url};

use crate::config::{Config, EthRpcConfig, StorageConfig};

#[derive(Clone)]
pub struct AppState {
    eth_fetcher: Arc<EthFetcher>,
}

impl AppState {
    pub fn new(eth_fetcher: Arc<EthFetcher>) -> Self {
        Self { eth_fetcher }
    }

    pub async fn from_config(config: &Config) -> anyhow::Result<Self> {
        let rpc = &config.eth.rpc;
        let http_client = http_client(rpc)?;
        let rpc_url = Url::parse(&rpc.url)
            .with_context(|| format!("eth.rpc.url is not a valid URL: {}", rpc.url))?;

        let rpc_source = Arc::new(RpcTxSource::new(http_client, rpc_url, rpc.rps));
        let eth_tx_source = eth_tx_source(&config.storage, rpc_source.clone()).await?;
        let tx_classificator = Arc::new(FulliestEthTxClassificator::new(rpc_source));

        Ok(Self::new(Arc::new(EthFetcher::new(
            eth_tx_source,
            tx_classificator,
        ))))
    }

    pub fn eth_fetcher(&self) -> &EthFetcher {
        &self.eth_fetcher
    }
}

async fn eth_tx_source(
    storage: &StorageConfig,
    upstream: Arc<dyn EthTxSource>,
) -> anyhow::Result<Arc<dyn EthTxSource>> {
    if !storage.persist_txs {
        tracing::info!("storage.persist_txs is off, every request goes to the rpc");
        return Ok(upstream);
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

    Ok(Arc::new(StoringEthTxSource::new(
        upstream, repository, cache,
    )))
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
