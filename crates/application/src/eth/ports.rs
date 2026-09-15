use std::io;

use futures::stream::BoxStream;

use domain::eth::{EthAddress, EthTx};

#[async_trait::async_trait]
pub trait EthTxSource: Send + Sync {
    async fn txs(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> BoxStream<'_, Result<EthTx, io::Error>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockRef {
    #[default]
    Latest,
    Number(u64),
}

#[derive(Debug, Clone)]
pub struct EthLog {
    address: EthAddress,
    topics: Vec<[u8; 32]>,
    data: Vec<u8>,
}

impl EthLog {
    pub fn new(address: EthAddress, topics: Vec<[u8; 32]>, data: Vec<u8>) -> Self {
        Self {
            address,
            topics,
            data,
        }
    }

    pub fn address(&self) -> &EthAddress {
        &self.address
    }

    pub fn topics(&self) -> &[[u8; 32]] {
        &self.topics
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

#[derive(Debug, Clone)]
pub struct EthReceipt {
    succeeded: bool,
    contract_address: Option<EthAddress>,
    logs: Vec<EthLog>,
}

impl EthReceipt {
    pub fn new(succeeded: bool, contract_address: Option<EthAddress>, logs: Vec<EthLog>) -> Self {
        Self {
            succeeded,
            contract_address,
            logs,
        }
    }

    pub fn succeeded(&self) -> bool {
        self.succeeded
    }

    pub fn contract_address(&self) -> Option<&EthAddress> {
        self.contract_address.as_ref()
    }

    pub fn logs(&self) -> &[EthLog] {
        &self.logs
    }
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
