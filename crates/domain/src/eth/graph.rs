use alloy_primitives::{TxHash, U256};

use crate::eth::{EthAddress, EthReceipt, EthTx};

#[derive(Clone, Debug)]
pub enum ContractAction {
    Erc20Transfer {
        token: EthAddress,
        from: EthAddress,
        to: EthAddress,

        amount: U256,
        token_name: String,
        decimals: u8,
    },
    Other,
}

#[derive(Clone, Debug)]
pub struct NativeTransfer {
    from: EthAddress,
    to: EthAddress,

    amount: U256,
}

impl NativeTransfer {
    pub fn from(&self) -> &EthAddress {
        &self.from
    }

    pub fn to(&self) -> &EthAddress {
        &self.to
    }

    pub fn amount(&self) -> U256 {
        self.amount
    }
}

impl TryFrom<EthTx> for NativeTransfer {
    type Error = EthTx;

    fn try_from(tx: EthTx) -> Result<Self, Self::Error> {
        if tx.to.is_none() || !tx.data.is_empty() {
            return Err(tx);
        }
        Ok(Self {
            from: tx.from,
            to: tx.to.unwrap(),
            amount: tx.amount,
        })
    }
}

#[derive(Clone, Debug)]
pub enum InteractionKind {
    Protocol,
    NativeTransfer(NativeTransfer),
    ContractDeployment {
        contract_address: EthAddress,
        deployer: EthAddress,
    },
    ContractInteraction {
        contract_address: EthAddress,
        interactor: EthAddress,
        contract_interaction_type: ContractAction,
    },
}

impl InteractionKind {
    pub fn endpoints(&self) -> Option<(&EthAddress, &EthAddress)> {
        match self {
            InteractionKind::Protocol => None,
            InteractionKind::NativeTransfer(NativeTransfer { from, to, .. }) => Some((from, to)),
            InteractionKind::ContractDeployment {
                contract_address,
                deployer,
            } => Some((deployer, contract_address)),
            InteractionKind::ContractInteraction {
                contract_address,
                interactor,
                contract_interaction_type,
            } => match contract_interaction_type {
                ContractAction::Erc20Transfer { from, to, .. } => Some((from, to)),
                ContractAction::Other => Some((interactor, contract_address)),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct Interaction {
    receipt: EthReceipt,
    kind: InteractionKind,
}

impl Interaction {
    pub fn new(receipt: EthReceipt, kind: InteractionKind) -> Self {
        Self { receipt, kind }
    }

    pub fn receipt(&self) -> &EthReceipt {
        &self.receipt
    }

    pub fn kind(&self) -> &InteractionKind {
        &self.kind
    }

    pub fn endpoints(&self) -> Option<(&EthAddress, &EthAddress)> {
        self.kind.endpoints()
    }
}

#[derive(Clone, Debug)]
pub struct TxMeta {
    tx_hash: TxHash,
    block_number: u64,
    timestamp: u64,
}

impl TxMeta {
    pub fn tx_hash(&self) -> &TxHash {
        &self.tx_hash
    }

    pub fn block_number(&self) -> u64 {
        self.block_number
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

impl From<&EthTx> for TxMeta {
    fn from(tx: &EthTx) -> Self {
        Self {
            tx_hash: tx.tx_hash,
            block_number: tx.block_number,
            timestamp: tx.timestamp,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InteractionId {
    tx_hash: TxHash,
    slot: u32,
}

impl InteractionId {
    pub fn new(tx_hash: TxHash, slot: u32) -> Self {
        Self { tx_hash, slot }
    }

    pub fn tx_hash(&self) -> &TxHash {
        &self.tx_hash
    }

    pub fn slot(&self) -> u32 {
        self.slot
    }
}

#[derive(Clone, Debug)]
pub struct InteractionEdge {
    meta: TxMeta,
    slot: u32,
    interaction: Interaction,
}

impl InteractionEdge {
    pub fn new(meta: TxMeta, slot: u32, interaction: Interaction) -> Self {
        Self {
            meta,
            slot,
            interaction,
        }
    }

    pub fn meta(&self) -> &TxMeta {
        &self.meta
    }

    pub fn slot(&self) -> u32 {
        self.slot
    }

    pub fn id(&self) -> InteractionId {
        InteractionId::new(self.meta.tx_hash, self.slot)
    }

    pub fn interaction(&self) -> &Interaction {
        &self.interaction
    }

    pub fn endpoints(&self) -> Option<(&EthAddress, &EthAddress)> {
        self.interaction.endpoints()
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Bytes, TxHash};

    use super::*;
    use crate::eth::{EthReceipt, EthTx};

    fn tx(last_byte: u8) -> EthTx {
        EthTx::builder()
            .tx_hash(TxHash::with_last_byte(last_byte))
            .block_number(21_000_000)
            .timestamp(1_737_000_000)
            .amount(U256::from(1_u64))
            .from(EthAddress::from([1u8; 20]))
            .to(EthAddress::from([2u8; 20]))
            .data(Bytes::new())
            .build()
    }

    fn erc20_edge(tx: &EthTx, slot: u32, amount: u64) -> InteractionEdge {
        let interaction = Interaction::new(
            EthReceipt::new(true, None, Vec::new()),
            InteractionKind::ContractInteraction {
                contract_address: EthAddress::from([3u8; 20]),
                interactor: EthAddress::from([1u8; 20]),
                contract_interaction_type: ContractAction::Erc20Transfer {
                    token: EthAddress::from([3u8; 20]),
                    from: EthAddress::from([1u8; 20]),
                    to: EthAddress::from([2u8; 20]),
                    amount: U256::from(amount),
                    token_name: "USDC".to_owned(),
                    decimals: 6,
                },
            },
        );

        InteractionEdge::new(TxMeta::from(tx), slot, interaction)
    }

    #[test]
    fn every_slot_of_one_tx_keeps_its_own_id() {
        let tx = tx(1);

        assert_ne!(erc20_edge(&tx, 0, 10).id(), erc20_edge(&tx, 1, 20).id());
    }

    #[test]
    fn the_same_slot_of_the_same_tx_is_the_same_edge() {
        let tx = tx(1);

        assert_eq!(erc20_edge(&tx, 0, 10).id(), erc20_edge(&tx, 0, 10).id());
    }

    #[test]
    fn two_txs_between_the_same_pair_stay_apart() {
        assert_ne!(
            erc20_edge(&tx(1), 0, 10).id(),
            erc20_edge(&tx(2), 0, 10).id()
        );
    }

    #[test]
    fn an_erc20_transfer_hangs_off_the_token_holders() {
        let edge = erc20_edge(&tx(1), 0, 10);
        let (from, to) = edge.endpoints().unwrap();

        assert_eq!(from, &EthAddress::from([1u8; 20]));
        assert_eq!(to, &EthAddress::from([2u8; 20]));
    }

    #[test]
    fn a_protocol_interaction_has_no_endpoints() {
        let interaction = Interaction::new(
            EthReceipt::new(true, None, Vec::new()),
            InteractionKind::Protocol,
        );
        let edge = InteractionEdge::new(TxMeta::from(&tx(1)), 0, interaction);

        assert!(edge.endpoints().is_none());
    }

    #[test]
    fn a_deployment_runs_from_the_deployer_to_the_contract() {
        let interaction = Interaction::new(
            EthReceipt::new(true, None, Vec::new()),
            InteractionKind::ContractDeployment {
                contract_address: EthAddress::from([7u8; 20]),
                deployer: EthAddress::from([1u8; 20]),
            },
        );
        let edge = InteractionEdge::new(TxMeta::from(&tx(1)), 0, interaction);
        let (from, to) = edge.endpoints().unwrap();

        assert_eq!(from, &EthAddress::from([1u8; 20]));
        assert_eq!(to, &EthAddress::from([7u8; 20]));
    }
}
