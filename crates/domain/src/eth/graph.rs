use std::collections::HashMap;

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

#[derive(Clone, Debug)]
pub struct InteractionEdge {
    meta: TxMeta,
    interaction: Interaction,
}

impl InteractionEdge {
    pub fn new(meta: TxMeta, interaction: Interaction) -> Self {
        Self { meta, interaction }
    }

    pub fn meta(&self) -> &TxMeta {
        &self.meta
    }

    pub fn interaction(&self) -> &Interaction {
        &self.interaction
    }
}

pub struct InteractionGraph {
    graph: StableGraph<EthAddress, InteractionEdge>,
    graph_index: HashMap<String, NodeIndex>,
}

impl InteractionGraph {
    pub fn new() -> Self {
        Self {
            graph: StableGraph::default(),
            graph_index: HashMap::new(),
        }
    }

    pub fn insert(&mut self, edge: InteractionEdge) -> bool {
        let Some((from, to)) = edge.interaction().endpoints() else {
            return false;
        };
        let (from, to) = (*from, *to);
        let node_from = self.get_or_create_node(&from);
        let node_to = self.get_or_create_node(&to);
        self.graph.add_edge(node_from, node_to, edge);
        true
    }

    fn get_or_create_node(&mut self, address: &EthAddress) -> NodeIndex {
        let Self { graph, graph_index } = self;
        *graph_index
            .entry(address.hex())
            .or_insert_with(|| graph.add_node(address.clone()))
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
