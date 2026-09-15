use std::sync::Arc;

use anyhow::Context;

use adapters::eth::RpcTxSource;
use application::eth::{EthFetcher, classificator::FulliestEthTxClassificator};
use reqwest::Proxy;

const DEFAULT_RPC_PROXY: &str = "socks5h://127.0.0.1:2080/";

#[derive(Clone)]
pub struct AppState {
    eth_fetcher: Arc<EthFetcher>,
}

impl AppState {
    pub fn new(eth_fetcher: Arc<EthFetcher>) -> Self {
        Self { eth_fetcher }
    }

    pub fn from_env() -> anyhow::Result<Self> {
        let proxy_url =
            std::env::var("ETH_RPC_PROXY").unwrap_or_else(|_| DEFAULT_RPC_PROXY.to_owned());
        let mut client = reqwest::ClientBuilder::new();
        if !proxy_url.is_empty() {
            client = client
                .proxy(Proxy::all(&proxy_url).context("ETH_RPC_PROXY is not a valid proxy URL")?);
        }
        let http_client = client.build().context("failed to build the HTTP client")?;

        let rpc_url = std::env::var("ETH_RPC_URL").context("ETH_RPC_URL must be set")?;
        let rpc_source = Arc::new(RpcTxSource::new(http_client, rpc_url));
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
