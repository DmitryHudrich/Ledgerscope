use crate::eth::EthAddress;

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
