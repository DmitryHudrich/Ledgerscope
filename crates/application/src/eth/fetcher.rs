use std::sync::Arc;

use futures::StreamExt;
use rangemap::RangeInclusiveSet;
use tokio::sync::Mutex;

use domain::eth::{EthAddress, EthTx, RawTxGraph};

use crate::eth::ports::EthTxSource;

pub struct GraphSnapshot {
    nodes: Vec<EthAddress>,
    edges: Vec<EthTx>,
}

impl GraphSnapshot {
    pub fn new(nodes: Vec<EthAddress>, edges: Vec<EthTx>) -> Self {
        Self { nodes, edges }
    }

    pub fn nodes(&self) -> &[EthAddress] {
        &self.nodes
    }

    pub fn edges(&self) -> &[EthTx] {
        &self.edges
    }
}

pub struct EthFetcher {
    graph: Mutex<RawTxGraph>,
    seeded_range: Mutex<RangeInclusiveSet<u64>>,

    eth_tx_source: Arc<dyn EthTxSource>,
}

impl EthFetcher {
    pub fn new(eth_tx_source: Arc<dyn EthTxSource>) -> Self {
        Self {
            eth_tx_source,
            seeded_range: Mutex::default(),
            graph: Default::default(),
        }
    }

    pub async fn seed_eth(&self, lower_block: u64, highest_block: u64) {
        self.eth_tx_source
            .txs(lower_block, highest_block)
            .await
            .for_each_concurrent(20, |tx| async move {
                match tx {
                    Ok(tx) => {
                        let block_number = tx.block_number();

                        let mut graph_lock = self.graph.lock().await;
                        graph_lock.insert(tx);
                        drop(graph_lock);

                        let mut seeded_lock = self.seeded_range.lock().await;
                        seeded_lock.insert(block_number..=block_number);
                    }
                    Err(e) => tracing::error!("{}", e),
                }
            })
            .await;
    }

    pub async fn full_graph(&self) -> GraphSnapshot {
        let graph_lock = self.graph.lock().await;
        let nodes = graph_lock.graph().node_weights().cloned().collect();
        let edges = graph_lock.graph().edge_weights().cloned().collect();
        GraphSnapshot::new(nodes, edges)
    }
}
