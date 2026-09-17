use std::collections::HashSet;

use alloy_primitives::{Bytes, TxHash, U256};
use bon::Builder;

use crate::eth::{EthAddress, EthReceipt};

#[derive(Eq, Clone, Debug, Hash, PartialEq, Builder)]
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

#[derive(Clone, Debug)]
pub struct MinedTx {
    tx: EthTx,
    receipt: EthReceipt,
}

impl MinedTx {
    pub fn new(tx: EthTx, receipt: EthReceipt) -> Self {
        Self { tx, receipt }
    }

    pub fn tx(&self) -> &EthTx {
        &self.tx
    }

    pub fn receipt(&self) -> &EthReceipt {
        &self.receipt
    }

    pub fn into_parts(self) -> (EthTx, EthReceipt) {
        (self.tx, self.receipt)
    }

    pub fn touches(&self, addresses: &HashSet<EthAddress>) -> bool {
        if addresses.contains(&self.tx.from) {
            return true;
        }

        if self.tx.to.is_some_and(|to| addresses.contains(&to)) {
            return true;
        }

        if self
            .receipt
            .contract_address()
            .is_some_and(|address| addresses.contains(address))
        {
            return true;
        }

        self.receipt.logs().iter().any(|log| {
            log.topics()
                .iter()
                .skip(1)
                .any(|topic| addresses.contains(&EthAddress::from_word(*topic)))
        })
    }
}
