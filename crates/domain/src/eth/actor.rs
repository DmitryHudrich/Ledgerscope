use std::hash::{Hash, Hasher};

use crate::eth::{AddressLabel, EthAddress};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractKind {
    Erc20 { symbol: String, decimals: u8 },
    Plain,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActorKind {
    Eoa,
    Contract(ContractKind),
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActorHint {
    Eoa,
    Contract,
    Erc20,
}

#[derive(Clone, Debug)]
pub struct Actor {
    address: EthAddress,
    labels: Vec<AddressLabel>,
    kind: ActorKind,
}

impl Actor {
    pub fn new(address: EthAddress, kind: ActorKind, labels: Vec<AddressLabel>) -> Self {
        Self {
            address,
            kind,
            labels,
        }
    }

    pub fn unknown(address: EthAddress) -> Self {
        Self::new(address, ActorKind::Unknown, Vec::new())
    }

    pub fn labeled(self, labels: Vec<AddressLabel>) -> Self {
        Self { labels, ..self }
    }

    pub fn address(&self) -> &EthAddress {
        &self.address
    }

    pub fn kind(&self) -> &ActorKind {
        &self.kind
    }

    pub fn labels(&self) -> &[AddressLabel] {
        &self.labels
    }

    pub fn is_contract(&self) -> bool {
        matches!(self.kind, ActorKind::Contract(_))
    }

    pub fn erc20(&self) -> Option<(&str, u8)> {
        match &self.kind {
            ActorKind::Contract(ContractKind::Erc20 { symbol, decimals }) => {
                Some((symbol.as_str(), *decimals))
            }
            _ => None,
        }
    }
}

impl PartialEq for Actor {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
    }
}

impl Eq for Actor {}

impl Hash for Actor {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.address.hash(state);
    }
}

impl ActorHint {
    pub fn outranks(self, other: Self) -> bool {
        self.rank() > other.rank()
    }

    fn rank(self) -> u8 {
        match self {
            ActorHint::Eoa => 0,
            ActorHint::Contract => 1,
            ActorHint::Erc20 => 2,
        }
    }
}
