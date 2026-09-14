use std::sync::Arc;

use anyhow::Context;

use adapters::eth::RpcTxSource;
use application::eth::EthFetcher;
use reqwest::Proxy;

#[derive(Clone)]
pub struct AppState {
    eth_fetcher: Arc<EthFetcher>,
}

impl AppState {
    pub fn new(eth_fetcher: Arc<EthFetcher>) -> Self {
        Self { eth_fetcher }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let http_client = reqwest::ClientBuilder::new()
            .proxy(Proxy::all("socks5h://127.0.0.1:2080/").unwrap())
            .build()
            .unwrap();
        let rpc_url = std::env::var("ETH_RPC_URL").context("ETH_RPC_URL must be set")?;
        let eth_tx_source = Arc::new(RpcTxSource::new(http_client, rpc_url));

        Ok(Self::new(Arc::new(EthFetcher::new(eth_tx_source))))
    }

    pub fn eth_fetcher(&self) -> &EthFetcher {
        &self.eth_fetcher
    }
}
