use std::hash::{Hash, Hasher};

use alloy_primitives::U256;

use crate::eth::{AddressLabel, EthAddress, MinedTx, TxMeta};

#[derive(Clone, Debug)]
pub struct LowLevelActor {
    address: EthAddress,
    labels: Vec<AddressLabel>,
}

impl LowLevelActor {
    pub fn new(address: EthAddress, labels: Vec<AddressLabel>) -> Self {
        Self { address, labels }
    }

    pub fn address(&self) -> &EthAddress {
        &self.address
    }

    pub fn labels(&self) -> &[AddressLabel] {
        &self.labels
    }
}

impl PartialEq for LowLevelActor {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
    }
}

impl Eq for LowLevelActor {}

impl Hash for LowLevelActor {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.address.hash(state);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LowLevelTarget {
    Account(EthAddress),
    Deployment(EthAddress),
}

impl LowLevelTarget {
    pub fn address(&self) -> &EthAddress {
        match self {
            LowLevelTarget::Account(address) | LowLevelTarget::Deployment(address) => address,
        }
    }

    pub fn is_deployment(&self) -> bool {
        matches!(self, LowLevelTarget::Deployment(_))
    }
}

#[derive(Clone, Debug)]
pub struct LowLevelInteraction {
    from: EthAddress,
    target: LowLevelTarget,
    meta: TxMeta,
    amount: U256,
    succeeded: bool,
}

impl LowLevelInteraction {
    pub fn new(
        from: EthAddress,
        target: LowLevelTarget,
        meta: TxMeta,
        amount: U256,
        succeeded: bool,
    ) -> Self {
        Self {
            from,
            target,
            meta,
            amount,
            succeeded,
        }
    }

    pub fn from(&self) -> &EthAddress {
        &self.from
    }

    pub fn target(&self) -> &LowLevelTarget {
        &self.target
    }

    pub fn to(&self) -> &EthAddress {
        self.target.address()
    }

    pub fn is_deployment(&self) -> bool {
        self.target.is_deployment()
    }

    pub fn meta(&self) -> &TxMeta {
        &self.meta
    }

    pub fn amount(&self) -> U256 {
        self.amount
    }

    pub fn succeeded(&self) -> bool {
        self.succeeded
    }

    pub fn endpoints(&self) -> (&EthAddress, &EthAddress) {
        (&self.from, self.target.address())
    }
}

impl TryFrom<&MinedTx> for LowLevelInteraction {
    type Error = NowhereToPointAt;

    fn try_from(mined: &MinedTx) -> Result<Self, Self::Error> {
        let tx = mined.tx();
        let receipt = mined.receipt();

        let target = match tx.to() {
            Some(to) => LowLevelTarget::Account(*to),
            None => receipt
                .contract_address()
                .map(|created| LowLevelTarget::Deployment(*created))
                .ok_or(NowhereToPointAt)?,
        };

        Ok(Self::new(
            *tx.from(),
            target,
            TxMeta::from(tx),
            tx.amount(),
            receipt.succeeded(),
        ))
    }
}

#[derive(Debug)]
pub struct NowhereToPointAt;

impl std::fmt::Display for NowhereToPointAt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a deployment that never named its contract")
    }
}

impl std::error::Error for NowhereToPointAt {}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Bytes, TxHash};

    use super::*;
    use crate::eth::{EthReceipt, EthTx};

    fn address(last_byte: u8) -> EthAddress {
        EthAddress::from([last_byte; 20])
    }

    fn tx(to: Option<EthAddress>) -> EthTx {
        let builder = EthTx::builder()
            .tx_hash(TxHash::with_last_byte(1))
            .block_number(21_000_000)
            .timestamp(1_737_000_000)
            .amount(U256::from(7_u64))
            .from(address(1))
            .data(Bytes::new());

        match to {
            Some(to) => builder.to(to).build(),
            None => builder.build(),
        }
    }

    #[test]
    fn a_plain_call_runs_from_the_sender_to_the_callee() {
        let mined = MinedTx::new(
            tx(Some(address(2))),
            EthReceipt::new(true, None, Vec::new()),
        );
        let interaction = LowLevelInteraction::try_from(&mined).unwrap();

        assert_eq!(interaction.endpoints(), (&address(1), &address(2)));
        assert!(!interaction.is_deployment());
        assert_eq!(interaction.amount(), U256::from(7_u64));
        assert!(interaction.succeeded());
    }

    #[test]
    fn a_deployment_points_at_the_contract_it_made() {
        let mined = MinedTx::new(
            tx(None),
            EthReceipt::new(true, Some(address(9)), Vec::new()),
        );
        let interaction = LowLevelInteraction::try_from(&mined).unwrap();

        assert_eq!(interaction.endpoints(), (&address(1), &address(9)));
        assert!(interaction.is_deployment());
    }

    #[test]
    fn a_deployment_without_a_contract_makes_no_edge() {
        let mined = MinedTx::new(tx(None), EthReceipt::new(true, None, Vec::new()));

        assert!(LowLevelInteraction::try_from(&mined).is_err());
    }

    #[test]
    fn a_failed_tx_still_draws_its_edge() {
        let mined = MinedTx::new(
            tx(Some(address(2))),
            EthReceipt::new(false, None, Vec::new()),
        );
        let interaction = LowLevelInteraction::try_from(&mined).unwrap();

        assert!(!interaction.succeeded());
        assert_eq!(interaction.endpoints(), (&address(1), &address(2)));
    }

    #[test]
    fn an_actor_is_the_same_actor_whatever_we_label_it() {
        let bare = LowLevelActor::new(address(1), Vec::new());
        let tagged = LowLevelActor::new(
            address(1),
            vec![AddressLabel::new(
                "binance".to_owned(),
                "etherscan.io".to_owned(),
            )],
        );

        assert_eq!(bare, tagged);
        assert!(bare.labels().is_empty());
        assert_eq!(tagged.labels()[0].value(), "binance");
        assert_eq!(tagged.labels()[0].source(), "etherscan.io");
    }

    #[test]
    fn two_addresses_are_two_actors() {
        assert_ne!(
            LowLevelActor::new(address(1), Vec::new()),
            LowLevelActor::new(address(2), Vec::new())
        );
    }
}
