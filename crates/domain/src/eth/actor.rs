use std::hash::{Hash, Hasher};

use crate::eth::EthAddress;

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
    kind: ActorKind,
}

impl Actor {
    pub fn new(address: EthAddress, kind: ActorKind) -> Self {
        Self { address, kind }
    }

    pub fn unknown(address: EthAddress) -> Self {
        Self::new(address, ActorKind::Unknown)
    }

    pub fn address(&self) -> &EthAddress {
        &self.address
    }

    pub fn kind(&self) -> &ActorKind {
        &self.kind
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

#[cfg(test)]
mod tests {
    use super::*;

    fn address(last_byte: u8) -> EthAddress {
        EthAddress::from([last_byte; 20])
    }

    fn usdc() -> ActorKind {
        ActorKind::Contract(ContractKind::Erc20 {
            symbol: "USDC".to_owned(),
            decimals: 6,
        })
    }

    #[test]
    fn an_actor_is_the_same_actor_whatever_we_learned_about_it() {
        let guessed = Actor::unknown(address(1));
        let known = Actor::new(address(1), usdc());

        assert_eq!(guessed, known);
    }

    #[test]
    fn two_addresses_are_two_actors() {
        assert_ne!(Actor::unknown(address(1)), Actor::unknown(address(2)));
    }

    #[test]
    fn only_a_token_tells_its_symbol() {
        let token = Actor::new(address(1), usdc());

        assert_eq!(token.erc20(), Some(("USDC", 6)));
        assert!(token.is_contract());
        assert_eq!(Actor::unknown(address(2)).erc20(), None);
    }

    #[test]
    fn a_plain_contract_is_still_a_contract() {
        let contract = Actor::new(address(1), ActorKind::Contract(ContractKind::Plain));

        assert!(contract.is_contract());
        assert_eq!(contract.erc20(), None);
    }

    #[test]
    fn a_proven_contract_beats_a_guessed_wallet() {
        assert!(ActorHint::Contract.outranks(ActorHint::Eoa));
        assert!(ActorHint::Erc20.outranks(ActorHint::Contract));
        assert!(!ActorHint::Eoa.outranks(ActorHint::Contract));
    }
}
