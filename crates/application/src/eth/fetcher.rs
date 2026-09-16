use std::{
    ops::RangeInclusive,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use futures::StreamExt;
use rangemap::RangeInclusiveSet;
use tokio::sync::Mutex;

use domain::eth::{EthAddress, InteractionEdge, InteractionGraph, MinedTx, Placement, TxMeta};

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
        if lower_block > highest_block {
            return;
        }

        for range in self.unseeded_ranges(lower_block, highest_block).await {
            self.seed_range(range).await;
        }
    }

    async fn unseeded_ranges(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> Vec<RangeInclusive<u64>> {
        let wanted = lower_block..=highest_block;
        let seeded_lock = self.seeded_range.lock().await;

        seeded_lock.gaps(&wanted).collect()
    }

    async fn seed_range(&self, range: RangeInclusive<u64>) {
        let stumbled = AtomicBool::new(false);

        self.eth_tx_source
            .txs(*range.start(), *range.end())
            .await
            .for_each_concurrent(20, |mined| {
                let stumbled = &stumbled;

                async move {
                    match mined {
                        Ok(mined) => self.absorb(mined).await,
                        Err(e) => {
                            stumbled.store(true, Ordering::Relaxed);
                            tracing::error!("{}", e);
                        }
                    }
                }
            })
            .await;

        if stumbled.load(Ordering::Relaxed) {
            tracing::warn!(
                "blocks {}..={} stay unseeded, the next request will ask for them again",
                range.start(),
                range.end()
            );
            return;
        }

        let mut seeded_lock = self.seeded_range.lock().await;
        seeded_lock.insert(range);
    }

    async fn absorb(&self, mined: MinedTx) {
        let meta = TxMeta::from(mined.tx());

        let interactions = match self.tx_classificator.classificate(mined).await {
            Ok(interactions) => interactions,
            Err(e) => {
                tracing::error!("skipping {}: {}", meta.tx_hash(), e);
                return;
            }
        };

        let mut graph_lock = self.graph.lock().await;
        for (slot, interaction) in (0u32..).zip(interactions) {
            match graph_lock.insert(InteractionEdge::new(meta.clone(), slot, interaction)) {
                Placement::Drawn => {}
                Placement::AlreadyDrawn => {
                    tracing::debug!("{} #{} is already drawn", meta.tx_hash(), slot)
                }
                Placement::Endpointless => {
                    tracing::debug!("{} #{} has no endpoints to draw", meta.tx_hash(), slot)
                }
            }
        }
    }

    pub async fn full_graph(&self) -> GraphSnapshot {
        let graph_lock = self.graph.lock().await;
        let nodes = graph_lock.graph().node_weights().cloned().collect();
        let edges = graph_lock.graph().edge_weights().cloned().collect();
        GraphSnapshot::new(nodes, edges)
    }
}

#[cfg(test)]
mod tests {
    use std::{io, sync::atomic::AtomicUsize};

    use alloy_primitives::{Bytes, TxHash, U256};
    use domain::eth::{ContractAction, EthReceipt, EthTx, Interaction, InteractionKind, MinedTx};

    use crate::{BoxStream, eth::classificator::ClassificateError};

    use super::*;

    fn address(last_byte: u8) -> EthAddress {
        EthAddress::from([last_byte; 20])
    }

    fn mined(block_number: u64) -> MinedTx {
        let tx = EthTx::builder()
            .tx_hash(TxHash::with_last_byte(block_number as u8))
            .block_number(block_number)
            .timestamp(block_number * 12)
            .amount(U256::ZERO)
            .from(address(1))
            .to(address(9))
            .data(Bytes::from_static(&[0xa9, 0x05, 0x9c, 0xbb]))
            .build();

        MinedTx::new(tx, EthReceipt::new(true, None, Vec::new()))
    }

    #[derive(Default)]
    struct CountingSource {
        asked: Mutex<Vec<(u64, u64)>>,
    }

    #[async_trait::async_trait]
    impl EthTxSource for CountingSource {
        async fn txs(
            &self,
            lower_block: u64,
            highest_block: u64,
        ) -> BoxStream<'_, Result<MinedTx, io::Error>> {
            self.asked.lock().await.push((lower_block, highest_block));
            let blocks: Vec<u64> = (lower_block..=highest_block).collect();

            Box::pin(async_stream::stream! {
                for block in blocks {
                    yield Ok(mined(block));
                }
            })
        }
    }

    #[derive(Default)]
    struct StumblingSource {
        attempts: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl EthTxSource for StumblingSource {
        async fn txs(
            &self,
            lower_block: u64,
            highest_block: u64,
        ) -> BoxStream<'_, Result<MinedTx, io::Error>> {
            let first = self.attempts.fetch_add(1, Ordering::Relaxed) == 0;
            let blocks: Vec<u64> = (lower_block..=highest_block).collect();

            Box::pin(async_stream::stream! {
                for block in blocks {
                    yield Ok(mined(block));
                }

                if first {
                    yield Err(io::Error::other("the node hung up"));
                }
            })
        }
    }

    struct TwoTransfers;

    #[async_trait::async_trait]
    impl EthTxClassificator for TwoTransfers {
        async fn classificate(
            &self,
            mined: MinedTx,
        ) -> Result<Vec<Interaction>, ClassificateError> {
            let receipt = mined.receipt().clone();
            let transfer = |amount: u64| {
                Interaction::new(
                    receipt.clone(),
                    InteractionKind::ContractInteraction {
                        contract_address: address(9),
                        interactor: address(1),
                        contract_interaction_type: ContractAction::Erc20Transfer {
                            token: address(9),
                            from: address(1),
                            to: address(2),
                            amount: U256::from(amount),
                            token_name: "USDC".to_owned(),
                            decimals: 6,
                        },
                    },
                )
            };

            Ok(vec![transfer(10), transfer(20)])
        }
    }

    async fn edges(fetcher: &EthFetcher) -> usize {
        fetcher.full_graph().await.edges().len()
    }

    #[tokio::test]
    async fn every_interaction_of_a_tx_reaches_the_graph() {
        let fetcher = EthFetcher::new(Arc::new(CountingSource::default()), Arc::new(TwoTransfers));

        fetcher.seed_eth(10, 12).await;

        assert_eq!(edges(&fetcher).await, 6);
    }

    #[tokio::test]
    async fn a_seeded_range_is_never_walked_again() {
        let source = Arc::new(CountingSource::default());
        let fetcher = EthFetcher::new(source.clone(), Arc::new(TwoTransfers));

        fetcher.seed_eth(10, 12).await;
        fetcher.seed_eth(10, 12).await;

        assert_eq!(edges(&fetcher).await, 6);
        assert_eq!(source.asked.lock().await.clone(), vec![(10, 12)]);
    }

    #[tokio::test]
    async fn only_the_gaps_are_seeded() {
        let source = Arc::new(CountingSource::default());
        let fetcher = EthFetcher::new(source.clone(), Arc::new(TwoTransfers));

        fetcher.seed_eth(11, 11).await;
        fetcher.seed_eth(14, 15).await;
        fetcher.seed_eth(10, 16).await;

        assert_eq!(edges(&fetcher).await, 14);
        assert_eq!(
            source.asked.lock().await.clone(),
            vec![(11, 11), (14, 15), (10, 10), (12, 13), (16, 16)]
        );
    }

    #[tokio::test]
    async fn a_range_that_stumbled_is_retried_without_doubling_its_edges() {
        let source = Arc::new(StumblingSource::default());
        let fetcher = EthFetcher::new(source.clone(), Arc::new(TwoTransfers));

        fetcher.seed_eth(10, 12).await;
        assert_eq!(edges(&fetcher).await, 6);

        fetcher.seed_eth(10, 12).await;
        assert_eq!(edges(&fetcher).await, 6);
        assert_eq!(source.attempts.load(Ordering::Relaxed), 2);

        fetcher.seed_eth(10, 12).await;
        assert_eq!(source.attempts.load(Ordering::Relaxed), 2);
    }
}
