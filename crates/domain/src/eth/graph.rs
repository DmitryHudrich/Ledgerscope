use std::collections::HashMap;

use petgraph::{graph::NodeIndex, prelude::StableGraph};

use crate::eth::{EthAddress, EthTx};

#[derive(Clone, Debug)]
pub enum ContractAction {
    Erc20Transfer {
        from: EthAddress,
        to: EthAddress,

        amount: u64,
        token_name: String,
    },
    Other,
}

#[derive(Clone, Debug)]
pub struct NativeTransfer {
    from: EthAddress,
    to: EthAddress,

    amount: u128,
}

impl NativeTransfer {
    pub fn from(&self) -> &EthAddress {
        &self.from
    }

    pub fn to(&self) -> &EthAddress {
        &self.to
    }

    pub fn amount(&self) -> u128 {
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
pub enum Interaction {
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

impl Interaction {
    fn endpoints(&self) -> (&EthAddress, &EthAddress) {
        match self {
            Interaction::Protocol => todo!(),
            Interaction::NativeTransfer(NativeTransfer { from, to, .. }) => (from, to),
            Interaction::ContractDeployment {
                contract_address,
                deployer,
            } => (deployer, contract_address),
            Interaction::ContractInteraction {
                contract_address,
                interactor,
                contract_interaction_type,
            } => match contract_interaction_type {
                ContractAction::Erc20Transfer { from, to, .. } => (from, to),
                ContractAction::Other => (interactor, contract_address),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct TxMeta {
    tx_hash: String,
    block_number: u64,
    timestamp: u64,
}

impl TxMeta {
    pub fn tx_hash(&self) -> &str {
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
            tx_hash: tx.tx_hash.clone(),
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

    pub fn insert(&mut self, edge: InteractionEdge) {
        let (from, to) = edge.interaction().endpoints();
        let node_from = self.get_or_create_node(from);
        let node_to = self.get_or_create_node(to);
        self.graph.add_edge(node_from, node_to, edge);
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
