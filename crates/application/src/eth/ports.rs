use std::io;

use futures::stream::BoxStream;

use domain::eth::EthTx;

#[async_trait::async_trait]
pub trait EthTxSource: Send + Sync {
    async fn txs(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> BoxStream<'_, Result<EthTx, io::Error>>;
}
