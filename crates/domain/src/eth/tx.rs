use alloy_primitives::{Bytes, TxHash, U256};
use bon::Builder;

use crate::eth::EthAddress;

#[derive(Eq, Clone, Hash, PartialEq, Builder)]
pub struct EthTx {
    pub(crate) tx_hash: TxHash,
    pub(crate) block_number: u64,
    pub(crate) timestamp: u64,

    pub(crate) amount: U256,

    pub(crate) from: EthAddress,
    pub(crate) to: Option<EthAddress>,
    pub(crate) data: Bytes,
}

impl EthTx {
    pub fn tx_hash(&self) -> &TxHash {
        &self.tx_hash
    }

    pub fn block_number(&self) -> u64 {
        self.block_number
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub fn amount(&self) -> U256 {
        self.amount
    }

    pub fn from(&self) -> &EthAddress {
        &self.from
    }

    pub fn to(&self) -> Option<&EthAddress> {
        self.to.as_ref()
    }

    pub fn data(&self) -> &Bytes {
        &self.data
    }
}
