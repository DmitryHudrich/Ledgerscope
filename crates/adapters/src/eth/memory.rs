use std::io;

use application::{BoxStream, eth::ports::EthTxSource};
use domain::eth::EthTx;

pub struct InMemoryTxSource {
    inner: Vec<EthTx>,
}

impl InMemoryTxSource {
    pub fn new(inner: Vec<EthTx>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl EthTxSource for InMemoryTxSource {
    async fn txs(
        &self,
        _lower_block: u64,
        _highest_block: u64,
    ) -> BoxStream<'_, Result<EthTx, io::Error>> {
        Box::pin(async_stream::stream! {
            for tx in &self.inner {
                yield Ok(tx.clone());
            }
        })
    }
}
