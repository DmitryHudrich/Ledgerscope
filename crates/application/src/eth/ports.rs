use std::{collections::HashMap, io};

use alloy_primitives::{Bytes, TxHash};
use futures::stream::BoxStream;

use domain::eth::{
    Actor, ActorHint, BlockBucket, BlockRange, BlockRef, EthAddress, EthReceipt, IndexCoverage,
    MinedTx,
};

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
pub trait EthTxIndex: Send + Sync {
    fn persistent(&self) -> bool;

    async fn coverage(&self, span: BlockRange) -> Result<IndexCoverage, io::Error>;

    async fn histogram(
        &self,
        span: BlockRange,
        buckets: u32,
    ) -> Result<Vec<BlockBucket>, io::Error>;

    async fn txs_touching(
        &self,
        addresses: &[EthAddress],
        span: BlockRange,
    ) -> Result<Vec<MinedTx>, io::Error>;
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
pub trait ActorRepository: Send + Sync {
    async fn actors(&self, addresses: &[EthAddress]) -> Result<Vec<Actor>, io::Error>;

    async fn remember(&self, actors: &[Actor]) -> Result<(), io::Error>;
}

#[async_trait::async_trait]
pub trait ActorResolver: Send + Sync {
    async fn resolve(
        &self,
        wanted: HashMap<EthAddress, Option<ActorHint>>,
    ) -> HashMap<EthAddress, Actor>;
}

#[async_trait::async_trait]
pub trait EthRpcSource: Send + Sync {
    async fn head_block(&self) -> Result<u64, io::Error>;

    async fn call(&self, to: &EthAddress, data: &[u8], block: BlockRef)
    -> Result<Bytes, io::Error>;

    async fn code(&self, address: &EthAddress, block: BlockRef) -> Result<Bytes, io::Error>;

    async fn receipt(&self, tx_hash: &TxHash) -> Result<Option<EthReceipt>, io::Error>;
}
