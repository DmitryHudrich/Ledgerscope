use std::{collections::HashSet, io, sync::Arc};

use futures::StreamExt;
use tokio::sync::Mutex;

use domain::eth::{BlockBucket, BlockRange, EthAddress, IndexCoverage, MinedTx};

use crate::eth::ports::{EthTxIndex, EthTxSource};

pub struct FetchingTxIndex {
    source: Arc<dyn EthTxSource>,
    held: Mutex<Option<(BlockRange, Arc<Vec<MinedTx>>)>>,
}

impl FetchingTxIndex {
    pub fn new(source: Arc<dyn EthTxSource>) -> Self {
        Self {
            source,
            held: Mutex::default(),
        }
    }

    async fn span_txs(&self, span: BlockRange) -> Result<Arc<Vec<MinedTx>>, io::Error> {
        if let Some((held, txs)) = self.held.lock().await.as_ref()
            && *held == span
        {
            return Ok(txs.clone());
        }

        let mut stream = self.source.txs(span.from_block(), span.to_block()).await;
        let mut txs = Vec::new();

        while let Some(item) = stream.next().await {
            txs.push(item?);
        }

        drop(stream);

        let txs = Arc::new(txs);
        *self.held.lock().await = Some((span, txs.clone()));

        Ok(txs)
    }
}

#[async_trait::async_trait]
impl EthTxIndex for FetchingTxIndex {
    fn persistent(&self) -> bool {
        false
    }

    async fn coverage(&self, _span: BlockRange) -> Result<IndexCoverage, io::Error> {
        Ok(IndexCoverage::default())
    }

    async fn histogram(
        &self,
        _span: BlockRange,
        _buckets: u32,
    ) -> Result<Vec<BlockBucket>, io::Error> {
        Ok(Vec::new())
    }

    async fn txs_touching(
        &self,
        addresses: &[EthAddress],
        span: BlockRange,
    ) -> Result<Vec<MinedTx>, io::Error> {
        let wanted: HashSet<EthAddress> = addresses.iter().copied().collect();

        Ok(self
            .span_txs(span)
            .await?
            .iter()
            .filter(|mined| mined.touches(&wanted))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use alloy_primitives::{Bytes, TxHash, U256};
    use domain::eth::{EthReceipt, EthTx};

    use crate::BoxStream;

    use super::*;

    fn address(last_byte: u8) -> EthAddress {
        EthAddress::from([last_byte; 20])
    }

    fn transfer(block: u64, from: u8, to: u8) -> MinedTx {
        let tx = EthTx::builder()
            .tx_hash(TxHash::with_last_byte(block as u8))
            .block_number(block)
            .timestamp(block * 12)
            .amount(U256::from(1_u64))
            .from(address(from))
            .to(address(to))
            .data(Bytes::new())
            .build();

        MinedTx::new(tx, EthReceipt::new(true, None, Vec::new()))
    }

    #[derive(Default)]
    struct CountingSource {
        calls: StdMutex<Vec<(u64, u64)>>,
    }

    #[async_trait::async_trait]
    impl EthTxSource for CountingSource {
        async fn txs(
            &self,
            lower_block: u64,
            highest_block: u64,
        ) -> BoxStream<'_, Result<MinedTx, io::Error>> {
            self.calls
                .lock()
                .unwrap()
                .push((lower_block, highest_block));

            Box::pin(async_stream::stream! {
                yield Ok(transfer(10, 1, 2));
                yield Ok(transfer(11, 3, 4));
            })
        }
    }

    #[tokio::test]
    async fn only_the_txs_of_the_asked_addresses_come_back() {
        let index = FetchingTxIndex::new(Arc::new(CountingSource::default()));

        let touching = index
            .txs_touching(&[address(3)], BlockRange::new(10, 11))
            .await
            .unwrap();

        assert_eq!(touching.len(), 1);
        assert_eq!(touching[0].tx().block_number(), 11);
    }

    #[tokio::test]
    async fn the_same_span_is_walked_once() {
        let source = Arc::new(CountingSource::default());
        let index = FetchingTxIndex::new(source.clone());

        index
            .txs_touching(&[address(1)], BlockRange::new(10, 11))
            .await
            .unwrap();
        index
            .txs_touching(&[address(3)], BlockRange::new(10, 11))
            .await
            .unwrap();

        assert_eq!(source.calls.lock().unwrap().clone(), vec![(10, 11)]);
    }

    #[tokio::test]
    async fn nothing_is_ever_indexed() {
        let index = FetchingTxIndex::new(Arc::new(CountingSource::default()));
        let coverage = index.coverage(BlockRange::new(10, 11)).await.unwrap();

        assert!(!index.persistent());
        assert_eq!(
            coverage.gaps(BlockRange::new(10, 11)),
            [BlockRange::new(10, 11)]
        );
    }
}
