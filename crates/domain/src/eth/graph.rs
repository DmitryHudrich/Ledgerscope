use std::collections::HashMap;

use petgraph::{graph::NodeIndex, prelude::StableGraph};

use crate::eth::{EthAddress, EthTx};

pub struct RawTxGraph {
    graph: StableGraph<EthAddress, EthTx>,
    graph_index: HashMap<String, NodeIndex>,
}

impl RawTxGraph {
    pub fn new() -> Self {
        Self {
            graph: StableGraph::default(),
            graph_index: HashMap::new(),
        }
    }

    pub fn insert(&mut self, tx: EthTx) {
        let node_from = self.get_or_create_node(tx.from());

        if let Some(to) = tx.to() {
            let node_to = self.get_or_create_node(to);
            self.graph.add_edge(node_from, node_to, tx);
        }
    }

    fn get_or_create_node(&mut self, addr: &EthAddress) -> NodeIndex {
        let hex = addr.hex();
        if let Some(&idx) = self.graph_index.get(&hex) {
            idx
        } else {
            let idx = self.graph.add_node(addr.clone());
            self.graph_index.insert(hex, idx);
            idx
        }
    }

    pub fn graph(&self) -> &StableGraph<EthAddress, EthTx> {
        &self.graph
    }

    pub fn graph_index(&self) -> &HashMap<String, NodeIndex> {
        &self.graph_index
    }
}

impl Default for RawTxGraph {
    fn default() -> Self {
        Self::new()
    }
}
