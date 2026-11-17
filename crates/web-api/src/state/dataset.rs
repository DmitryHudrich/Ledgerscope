use std::sync::Arc;

use application::eth::{StoringEthTxSource, ports::EthTxSource};

use crate::config::AwsPublicSection;

#[cfg(feature = "aws-public")]
pub fn source(
    config: &AwsPublicSection,
    http: &reqwest::Client,
    chain_id: u64,
    store: impl FnOnce(Arc<dyn EthTxSource>) -> Option<StoringEthTxSource>,
) -> Option<Arc<dyn EthTxSource>> {
    use adapters::eth::aws_public::{AwsPublicConfig, AwsPublicTxSource};
    use domain::eth::BlockSource;

    if !config.enabled || !config.chains.contains(&chain_id) {
        return None;
    }
    let base_url = match reqwest::Url::parse(&config.base_url) {
        Ok(url) => url,
        Err(error) => {
            tracing::warn!(
                "indexer.aws_public.base_url is not a url, the dataset stays off: {error}"
            );
            return None;
        }
    };

    let dataset = Arc::new(AwsPublicTxSource::new(
        http.clone(),
        AwsPublicConfig {
            base_url,
            prefix: config.prefix.clone(),
            cache_dir: config.cache_dir.clone().into(),
            keep_days: config.keep_days,
            window_blocks: config.window_blocks,
        },
    ));
    let Some(stored) = store(dataset) else {
        tracing::warn!("the aws public dataset needs storage.persist_txs, it stays off");
        return None;
    };
    tracing::info!("chain {chain_id} can take old blocks from the aws public blockchain dataset");
    Some(Arc::new(stored.with_source(BlockSource::Dataset)))
}

#[cfg(not(feature = "aws-public"))]
pub fn source(
    config: &AwsPublicSection,
    _http: &reqwest::Client,
    chain_id: u64,
    _store: impl FnOnce(Arc<dyn EthTxSource>) -> Option<StoringEthTxSource>,
) -> Option<Arc<dyn EthTxSource>> {
    if config.enabled && config.chains.contains(&chain_id) {
        tracing::warn!("this build has no aws-public feature, indexer.aws_public is ignored");
    }
    None
}
