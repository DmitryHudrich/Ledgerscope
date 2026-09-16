use std::io;

use alloy_primitives::{Bytes, TxHash};
use futures::stream::BoxStream;

use domain::eth::{BlockRef, EthAddress, EthReceipt, MinedTx};

#[async_trait::async_trait]
pub trait EthTxSource: Send + Sync {
    async fn txs(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> BoxStream<'_, Result<MinedTx, io::Error>>;
}

#[async_trait::async_trait]
pub trait EthTxRepository: Send + Sync {
    async fn indexed_blocks(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> Result<Vec<u64>, io::Error>;

    async fn txs(&self, lower_block: u64, highest_block: u64) -> Result<Vec<MinedTx>, io::Error>;

    async fn save(
        &self,
        lower_block: u64,
        highest_block: u64,
        txs: &[MinedTx],
    ) -> Result<(), io::Error>;
}

#[async_trait::async_trait]
pub trait EthTxCache: Send + Sync {
    async fn indexed_blocks(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> Result<Vec<u64>, io::Error>;

    async fn remember_indexed(&self, blocks: &[u64]) -> Result<(), io::Error>;
}

#[async_trait::async_trait]
pub trait EthRpcSource: Send + Sync {
    async fn call(&self, to: &EthAddress, data: &[u8], block: BlockRef)
    -> Result<Bytes, io::Error>;

    async fn code(&self, address: &EthAddress, block: BlockRef) -> Result<Bytes, io::Error>;

    async fn receipt(&self, tx_hash: &TxHash) -> Result<Option<EthReceipt>, io::Error>;
}
