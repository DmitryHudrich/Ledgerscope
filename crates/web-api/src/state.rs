use std::sync::Arc;

use anyhow::Context;

use adapters::eth::RpcTxSource;
use application::eth::{EthFetcher, classificator::FulliestEthTxClassificator};
use reqwest::Proxy;

use crate::config::{Config, EthRpcConfig};

#[derive(Clone)]
pub struct AppState {
    eth_fetcher: Arc<EthFetcher>,
}

impl AppState {
    pub fn new(eth_fetcher: Arc<EthFetcher>) -> Self {
        Self { eth_fetcher }
    }

    pub fn from_config(config: &Config) -> anyhow::Result<Self> {
        let rpc = &config.eth.rpc;
        let http_client = http_client(rpc)?;

        let rpc_source =
            Arc::new(RpcTxSource::new(http_client, rpc.url.clone()).with_rate_limit(rpc.rps));
        let eth_tx_source = rpc_source.clone();
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
