use std::io;

use application::{BoxStream, eth::ports::EthTxSource};
use domain::eth::MinedTx;

pub struct InMemoryTxSource {
    inner: Vec<MinedTx>,
}

impl InMemoryTxSource {
    pub fn new(inner: Vec<MinedTx>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl EthTxSource for InMemoryTxSource {
    async fn txs(
        &self,
        _lower_block: u64,
        _highest_block: u64,
    ) -> BoxStream<'_, Result<MinedTx, io::Error>> {
        Box::pin(async_stream::stream! {
            for tx in &self.inner {
                yield Ok(tx.clone());
            }
        })
    }
}
