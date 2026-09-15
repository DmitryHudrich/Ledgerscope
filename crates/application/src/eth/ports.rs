use std::io;

use futures::stream::BoxStream;

use domain::eth::{BlockRef, EthAddress, EthReceipt, EthTx};

#[async_trait::async_trait]
pub trait EthTxSource: Send + Sync {
    async fn txs(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> BoxStream<'_, Result<EthTx, io::Error>>;
}

#[async_trait::async_trait]
pub trait EthRpcSource: Send + Sync {
    async fn call(
        &self,
        to: &EthAddress,
        data: &[u8],
        block: BlockRef,
    ) -> Result<Vec<u8>, io::Error>;

    async fn code(&self, address: &EthAddress, block: BlockRef) -> Result<Vec<u8>, io::Error>;

    async fn receipt(&self, tx_hash: &str) -> Result<Option<EthReceipt>, io::Error>;
}
