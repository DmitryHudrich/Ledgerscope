use std::sync::Arc;

use futures::StreamExt;
use rangemap::RangeInclusiveSet;
use tokio::sync::Mutex;

use domain::eth::{EthAddress, InteractionEdge, InteractionGraph, TxMeta};

use crate::eth::{classificator::EthTxClassificator, ports::EthTxSource};

pub struct GraphSnapshot {
    nodes: Vec<EthAddress>,
    edges: Vec<InteractionEdge>,
}

impl GraphSnapshot {
    pub fn new(nodes: Vec<EthAddress>, edges: Vec<InteractionEdge>) -> Self {
        Self { nodes, edges }
    }

    pub fn nodes(&self) -> &[EthAddress] {
        &self.nodes
    }

    pub fn edges(&self) -> &[InteractionEdge] {
        &self.edges
    }
}

pub struct EthFetcher {
    graph: Mutex<InteractionGraph>,
    seeded_range: Mutex<RangeInclusiveSet<u64>>,

    eth_tx_source: Arc<dyn EthTxSource>,
    tx_classificator: Arc<dyn EthTxClassificator>,
}

impl EthFetcher {
    pub fn new(
        eth_tx_source: Arc<dyn EthTxSource>,
        tx_classificator: Arc<dyn EthTxClassificator>,
    ) -> Self {
        Self {
            eth_tx_source,
            seeded_range: Mutex::default(),
            graph: Default::default(),
            tx_classificator,
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
                        let meta = TxMeta::from(&tx);

                        let mut graph_lock = self.graph.lock().await;
                        let interaction = self.tx_classificator.classificate(tx).expect("Later");
                        graph_lock.insert(InteractionEdge::new(meta, interaction));
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
