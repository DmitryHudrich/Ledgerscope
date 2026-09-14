use bon::Builder;

use crate::eth::EthAddress;

#[derive(Eq, Clone, Hash, PartialEq, Builder)]
pub struct EthTx {
    tx_hash: String,
    block_number: u64,
    timestamp: u64,

    amount: u128,

    from: EthAddress,
    to: Option<EthAddress>,
    data: Vec<u8>,
}

impl EthTx {
    pub fn tx_hash(&self) -> &str {
        &self.tx_hash
    }

    pub fn block_number(&self) -> u64 {
        self.block_number
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub fn amount(&self) -> u128 {
        self.amount
    }

    pub fn from(&self) -> &EthAddress {
        &self.from
    }

    pub fn to(&self) -> Option<&EthAddress> {
        self.to.as_ref()
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
