use std::collections::{HashMap, HashSet};

use alloy_primitives::{TxHash, U256};
use petgraph::{graph::NodeIndex, prelude::StableGraph};

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
    fn endpoints(&self) -> Option<(&EthAddress, &EthAddress)> {
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

    fn endpoints(&self) -> Option<(&EthAddress, &EthAddress)> {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Placement {
    Drawn,
    AlreadyDrawn,
    Endpointless,
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
}

pub struct InteractionGraph {
    graph: StableGraph<EthAddress, InteractionEdge>,
    graph_index: HashMap<String, NodeIndex>,
    drawn: HashSet<InteractionId>,
}

impl InteractionGraph {
    pub fn new() -> Self {
        Self {
            graph: StableGraph::default(),
            graph_index: HashMap::new(),
            drawn: HashSet::new(),
        }
    }

    pub fn insert(&mut self, edge: InteractionEdge) -> Placement {
        let Some((from, to)) = edge.interaction().endpoints() else {
            return Placement::Endpointless;
        };
        let (from, to) = (*from, *to);

        if !self.drawn.insert(edge.id()) {
            return Placement::AlreadyDrawn;
        }

        let node_from = self.get_or_create_node(&from);
        let node_to = self.get_or_create_node(&to);
        self.graph.add_edge(node_from, node_to, edge);

        Placement::Drawn
    }

    fn get_or_create_node(&mut self, address: &EthAddress) -> NodeIndex {
        let Self {
            graph, graph_index, ..
        } = self;
        *graph_index
            .entry(address.hex())
            .or_insert_with(|| graph.add_node(*address))
    }

    pub fn graph(&self) -> &StableGraph<EthAddress, InteractionEdge> {
        &self.graph
    }

    pub fn graph_index(&self) -> &HashMap<String, NodeIndex> {
        &self.graph_index
    }
}

impl Default for InteractionGraph {
    fn default() -> Self {
        Self::new()
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
    fn every_slot_of_one_tx_gets_its_own_edge() {
        let tx = tx(1);
        let mut graph = InteractionGraph::new();

        assert_eq!(graph.insert(erc20_edge(&tx, 0, 10)), Placement::Drawn);
        assert_eq!(graph.insert(erc20_edge(&tx, 1, 20)), Placement::Drawn);
        assert_eq!(graph.graph().edge_count(), 2);
        assert_eq!(graph.graph().node_count(), 2);
    }

    #[test]
    fn the_same_slot_never_lands_twice() {
        let tx = tx(1);
        let mut graph = InteractionGraph::new();

        assert_eq!(graph.insert(erc20_edge(&tx, 0, 10)), Placement::Drawn);
        assert_eq!(
            graph.insert(erc20_edge(&tx, 0, 10)),
            Placement::AlreadyDrawn
        );
        assert_eq!(graph.graph().edge_count(), 1);
    }

    #[test]
    fn two_txs_between_the_same_pair_stay_apart() {
        let mut graph = InteractionGraph::new();

        assert_eq!(graph.insert(erc20_edge(&tx(1), 0, 10)), Placement::Drawn);
        assert_eq!(graph.insert(erc20_edge(&tx(2), 0, 10)), Placement::Drawn);
        assert_eq!(graph.graph().edge_count(), 2);
    }

    #[test]
    fn an_endpointless_interaction_is_turned_away() {
        let interaction = Interaction::new(
            EthReceipt::new(true, None, Vec::new()),
            InteractionKind::Protocol,
        );
        let edge = InteractionEdge::new(TxMeta::from(&tx(1)), 0, interaction);

        let mut graph = InteractionGraph::new();

        assert_eq!(graph.insert(edge), Placement::Endpointless);
        assert_eq!(graph.graph().edge_count(), 0);
    }
}
