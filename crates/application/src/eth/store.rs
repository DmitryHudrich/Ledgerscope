use std::{collections::HashSet, io, ops::RangeInclusive, sync::Arc};

use futures::StreamExt;
use rangemap::RangeInclusiveSet;

use domain::eth::MinedTx;

use crate::{
    BoxStream,
    eth::ports::{EthTxCache, EthTxRepository, EthTxSource},
};

pub struct StoringEthTxSource {
    upstream: Arc<dyn EthTxSource>,
    repository: Arc<dyn EthTxRepository>,
    cache: Option<Arc<dyn EthTxCache>>,
}

impl StoringEthTxSource {
    pub fn new(
        upstream: Arc<dyn EthTxSource>,
        repository: Arc<dyn EthTxRepository>,
        cache: Option<Arc<dyn EthTxCache>>,
    ) -> Self {
        Self {
            upstream,
            repository,
            cache,
        }
    }

    async fn indexed_blocks(&self, lower_block: u64, highest_block: u64) -> Vec<u64> {
        let cached = match &self.cache {
            Some(cache) => match cache.indexed_blocks(lower_block, highest_block).await {
                Ok(blocks) => blocks,
                Err(error) => {
                    tracing::warn!("cache lookup failed, asking the repository: {error}");
                    Vec::new()
                }
            },
            None => Vec::new(),
        };

        if covers(&cached, lower_block, highest_block) {
            return cached;
        }

        let stored = match self
            .repository
            .indexed_blocks(lower_block, highest_block)
            .await
        {
            Ok(blocks) => blocks,
            Err(error) => {
                tracing::warn!("repository lookup failed, falling back to the upstream: {error}");
                return cached;
            }
        };

        self.remember(&stored).await;

        stored
    }

    async fn remember(&self, blocks: &[u64]) {
        let Some(cache) = &self.cache else {
            return;
        };

        if blocks.is_empty() {
            return;
        }

        if let Err(error) = cache.remember_indexed(blocks).await {
            tracing::warn!("failed to cache indexed blocks: {error}");
        }
    }

    async fn persist(&self, range: &RangeInclusive<u64>, txs: &[MinedTx]) {
        if let Err(error) = self
            .repository
            .save(*range.start(), *range.end(), txs)
            .await
        {
            tracing::warn!(
                "failed to store blocks {}..={}: {error}",
                range.start(),
                range.end()
            );
            return;
        }

        let blocks: Vec<u64> = range.clone().collect();
        self.remember(&blocks).await;
    }
}

#[async_trait::async_trait]
impl EthTxSource for StoringEthTxSource {
    async fn txs(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> BoxStream<'_, Result<MinedTx, io::Error>> {
        let mut pending = RangeInclusiveSet::new();
        pending.insert(lower_block..=highest_block);

        let indexed = self.indexed_blocks(lower_block, highest_block).await;
        let mut stored = Vec::new();

        if !indexed.is_empty() {
            match self.repository.txs(lower_block, highest_block).await {
                Ok(txs) => {
                    for block in indexed {
                        pending.remove(block..=block);
                    }
                    stored = txs;
                }
                Err(error) => {
                    tracing::warn!(
                        "failed to read stored txs, falling back to the upstream: {error}"
                    )
                }
            }
        }

        let missing: Vec<RangeInclusive<u64>> = pending.into_iter().collect();

        Box::pin(async_stream::stream! {
            for tx in stored {
                yield Ok(tx);
            }

            for range in missing {
                let mut fetched = Vec::new();
                let mut failed = false;
                let mut upstream = self.upstream.txs(*range.start(), *range.end()).await;

                while let Some(item) = upstream.next().await {
                    match item {
                        Ok(tx) => {
                            fetched.push(tx.clone());
                            yield Ok(tx);
                        }
                        Err(error) => {
                            failed = true;
                            yield Err(error);
                        }
                    }
                }

                drop(upstream);

                if !failed {
                    self.persist(&range, &fetched).await;
                }
            }
        })
    }
}

fn covers(blocks: &[u64], lower_block: u64, highest_block: u64) -> bool {
    let wanted = highest_block.saturating_sub(lower_block) as usize + 1;
    let seen: HashSet<u64> = blocks
        .iter()
        .copied()
        .filter(|block| (lower_block..=highest_block).contains(block))
        .collect();

    seen.len() == wanted
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use alloy_primitives::{Bytes, TxHash, U256};
    use domain::eth::{EthAddress, EthReceipt, EthTx};

    use super::*;

    fn mined(block_number: u64, nonce: u8) -> MinedTx {
        let tx = EthTx::builder()
            .tx_hash(TxHash::with_last_byte(nonce))
            .block_number(block_number)
            .timestamp(block_number * 12)
            .amount(U256::from(nonce))
            .from(EthAddress::ZERO)
            .to(EthAddress::ZERO)
            .data(Bytes::new())
            .build();

        MinedTx::new(tx, EthReceipt::new(true, None, Vec::new()))
    }

    #[derive(Default)]
    struct FakeUpstream {
        calls: Mutex<Vec<(u64, u64)>>,
    }

    #[async_trait::async_trait]
    impl EthTxSource for FakeUpstream {
        async fn txs(
            &self,
            lower_block: u64,
            highest_block: u64,
        ) -> BoxStream<'_, Result<MinedTx, io::Error>> {
            self.calls
                .lock()
                .unwrap()
                .push((lower_block, highest_block));
            let blocks: Vec<u64> = (lower_block..=highest_block).collect();

            Box::pin(async_stream::stream! {
                for block in blocks {
                    yield Ok(mined(block, block as u8));
                }
            })
        }
    }

    #[derive(Default)]
    struct FakeRepository {
        stored: Mutex<Vec<MinedTx>>,
        indexed: Mutex<Vec<u64>>,
    }

    #[async_trait::async_trait]
    impl EthTxRepository for FakeRepository {
        async fn indexed_blocks(
            &self,
            lower_block: u64,
            highest_block: u64,
        ) -> Result<Vec<u64>, io::Error> {
            Ok(self
                .indexed
                .lock()
                .unwrap()
                .iter()
                .copied()
                .filter(|block| (lower_block..=highest_block).contains(block))
                .collect())
        }

        async fn txs(
            &self,
            lower_block: u64,
            highest_block: u64,
        ) -> Result<Vec<MinedTx>, io::Error> {
            let indexed = self.indexed.lock().unwrap().clone();

            Ok(self
                .stored
                .lock()
                .unwrap()
                .iter()
                .filter(|tx| {
                    let block = tx.tx().block_number();
                    (lower_block..=highest_block).contains(&block) && indexed.contains(&block)
                })
                .cloned()
                .collect())
        }

        async fn save(
            &self,
            lower_block: u64,
            highest_block: u64,
            txs: &[MinedTx],
        ) -> Result<(), io::Error> {
            self.stored.lock().unwrap().extend(txs.iter().cloned());
            self.indexed
                .lock()
                .unwrap()
                .extend(lower_block..=highest_block);

            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeCache {
        indexed: Mutex<Vec<u64>>,
        lookups: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl EthTxCache for FakeCache {
        async fn indexed_blocks(
            &self,
            lower_block: u64,
            highest_block: u64,
        ) -> Result<Vec<u64>, io::Error> {
            *self.lookups.lock().unwrap() += 1;

            Ok(self
                .indexed
                .lock()
                .unwrap()
                .iter()
                .copied()
                .filter(|block| (lower_block..=highest_block).contains(block))
                .collect())
        }

        async fn remember_indexed(&self, blocks: &[u64]) -> Result<(), io::Error> {
            self.indexed.lock().unwrap().extend_from_slice(blocks);

            Ok(())
        }
    }

    async fn collect(source: &StoringEthTxSource, lower: u64, highest: u64) -> Vec<u64> {
        let mut blocks: Vec<u64> = source
            .txs(lower, highest)
            .await
            .filter_map(|item| async move { item.ok() })
            .map(|tx| tx.tx().block_number())
            .collect()
            .await;

        blocks.sort_unstable();
        blocks
    }

    #[tokio::test]
    async fn the_upstream_answer_lands_in_the_repository() {
        let upstream = Arc::new(FakeUpstream::default());
        let repository = Arc::new(FakeRepository::default());
        let source = StoringEthTxSource::new(upstream.clone(), repository.clone(), None);

        assert_eq!(collect(&source, 10, 12).await, vec![10, 11, 12]);
        assert_eq!(repository.indexed.lock().unwrap().clone(), vec![10, 11, 12]);
        assert_eq!(repository.stored.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn a_stored_range_never_reaches_the_upstream() {
        let upstream = Arc::new(FakeUpstream::default());
        let repository = Arc::new(FakeRepository::default());
        let source = StoringEthTxSource::new(upstream.clone(), repository.clone(), None);

        collect(&source, 10, 12).await;
        upstream.calls.lock().unwrap().clear();

        assert_eq!(collect(&source, 10, 12).await, vec![10, 11, 12]);
        assert!(upstream.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_fully_cached_range_never_reaches_the_repository() {
        struct BlowingRepository(Arc<FakeRepository>);

        #[async_trait::async_trait]
        impl EthTxRepository for BlowingRepository {
            async fn indexed_blocks(&self, _: u64, _: u64) -> Result<Vec<u64>, io::Error> {
                panic!("the cache already knew the whole range");
            }

            async fn txs(
                &self,
                lower_block: u64,
                highest_block: u64,
            ) -> Result<Vec<MinedTx>, io::Error> {
                self.0.txs(lower_block, highest_block).await
            }

            async fn save(
                &self,
                lower_block: u64,
                highest_block: u64,
                txs: &[MinedTx],
            ) -> Result<(), io::Error> {
                self.0.save(lower_block, highest_block, txs).await
            }
        }

        let inner = Arc::new(FakeRepository::default());
        inner
            .save(20, 21, &[mined(20, 1), mined(21, 2)])
            .await
            .unwrap();

        let cache = Arc::new(FakeCache::default());
        cache.remember_indexed(&[20, 21]).await.unwrap();

        let upstream = Arc::new(FakeUpstream::default());
        let source = StoringEthTxSource::new(
            upstream.clone(),
            Arc::new(BlowingRepository(inner)),
            Some(cache.clone()),
        );

        assert_eq!(collect(&source, 20, 21).await, vec![20, 21]);
        assert!(upstream.calls.lock().unwrap().is_empty());
        assert_eq!(*cache.lookups.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn a_partly_cached_range_still_asks_the_repository() {
        let upstream = Arc::new(FakeUpstream::default());
        let repository = Arc::new(FakeRepository::default());
        let cache = Arc::new(FakeCache::default());
        let source =
            StoringEthTxSource::new(upstream.clone(), repository.clone(), Some(cache.clone()));

        collect(&source, 30, 31).await;
        upstream.calls.lock().unwrap().clear();

        assert_eq!(collect(&source, 30, 32).await, vec![30, 31, 32]);
        assert_eq!(upstream.calls.lock().unwrap().clone(), vec![(32, 32)]);
        assert_eq!(collect(&source, 30, 32).await, vec![30, 31, 32]);
        assert_eq!(upstream.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn only_the_gaps_are_asked_from_the_upstream() {
        let upstream = Arc::new(FakeUpstream::default());
        let repository = Arc::new(FakeRepository::default());
        let source = StoringEthTxSource::new(upstream.clone(), repository.clone(), None);

        collect(&source, 11, 11).await;
        collect(&source, 14, 15).await;
        upstream.calls.lock().unwrap().clear();

        assert_eq!(
            collect(&source, 10, 16).await,
            vec![10, 11, 12, 13, 14, 15, 16]
        );
        assert_eq!(
            upstream.calls.lock().unwrap().clone(),
            vec![(10, 10), (12, 13), (16, 16)]
        );
    }
}
