use std::{collections::HashMap, io, net::IpAddr, sync::Arc};

use domain::{
    eth::EthAddress,
    observe::{IpWalletLink, TlsObservation},
};

use crate::eth::ports::EthTxRepository;

#[derive(Clone, Copy, Debug)]
pub struct ObservedBroadcast {
    pub src_ip: IpAddr,
    pub observed_at: u64,
    pub confidence: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct LinkContribution {
    pub src_ip: IpAddr,
    pub chain_id: u64,
    pub address: EthAddress,
    pub contribution: f64,
    pub hits: u32,
    pub address_txs: u32,
    pub first_seen: u64,
    pub last_seen: u64,
}

#[async_trait::async_trait]
pub trait ObservationStore: Send + Sync {
    async fn insert(&self, observation: &TlsObservation) -> Result<(), io::Error>;

    async fn broadcasts_between(
        &self,
        chain_id: u64,
        from: u64,
        to: u64,
    ) -> Result<Vec<ObservedBroadcast>, io::Error>;

    async fn apply_link(&self, contribution: &LinkContribution) -> Result<(), io::Error>;

    async fn links(&self, chain_id: u64) -> Result<Vec<IpWalletLink>, io::Error>;

    async fn watermark(&self, chain_id: u64) -> Result<Option<u64>, io::Error>;

    async fn set_watermark(&self, chain_id: u64, block: u64) -> Result<(), io::Error>;

    async fn reset(&self, chain_id: u64) -> Result<(), io::Error>;
}

#[derive(Clone, Copy, Debug)]
pub struct TlsCorrelationConfig {
    pub window_secs: u64,
    pub skew_secs: u64,
}

impl Default for TlsCorrelationConfig {
    fn default() -> Self {
        Self {
            window_secs: 60,
            skew_secs: 5,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CorrelationRun {
    pub from_block: u64,
    pub to_block: u64,
    pub matched_pairs: usize,
    pub links_touched: usize,
}

pub struct TlsCorrelationService {
    store: Arc<dyn ObservationStore>,
    txs: Arc<dyn EthTxRepository>,
    config: TlsCorrelationConfig,
}

impl TlsCorrelationService {
    pub fn new(
        store: Arc<dyn ObservationStore>,
        txs: Arc<dyn EthTxRepository>,
        config: TlsCorrelationConfig,
    ) -> Self {
        Self { store, txs, config }
    }

    pub async fn record(&self, observation: &TlsObservation) -> Result<(), io::Error> {
        self.store.insert(observation).await
    }

    pub async fn links(&self, chain_id: u64) -> Result<Vec<IpWalletLink>, io::Error> {
        self.store.links(chain_id).await
    }

    pub async fn reset(&self, chain_id: u64) -> Result<(), io::Error> {
        self.store.reset(chain_id).await
    }

    pub async fn correlate(
        &self,
        chain_id: u64,
        from_block: Option<u64>,
        up_to_block: u64,
    ) -> Result<CorrelationRun, io::Error> {
        let from_block = match from_block {
            Some(from) => from,
            None => self.store.watermark(chain_id).await?.map_or(0, |w| w + 1),
        };
        if up_to_block < from_block {
            return Ok(CorrelationRun {
                from_block,
                to_block: up_to_block,
                ..CorrelationRun::default()
            });
        }

        let mut senders: Vec<(u64, EthAddress)> = Vec::new();
        for mined in self.txs.txs(from_block, up_to_block).await? {
            let tx = mined.tx();
            senders.push((tx.timestamp(), *tx.from()));
        }

        if senders.is_empty() {
            self.store.set_watermark(chain_id, up_to_block).await?;
            return Ok(CorrelationRun {
                from_block,
                to_block: up_to_block,
                ..CorrelationRun::default()
            });
        }

        senders.sort_by_key(|(timestamp, _)| *timestamp);
        let mut totals: HashMap<EthAddress, u32> = HashMap::new();
        for (_, address) in &senders {
            *totals.entry(*address).or_insert(0) += 1;
        }
        let span_from = senders
            .first()
            .map(|(t, _)| t.saturating_sub(self.config.window_secs))
            .unwrap_or(0);
        let span_to = senders
            .last()
            .map(|(t, _)| t + self.config.skew_secs)
            .unwrap_or(0);

        let mut broadcasts = self
            .store
            .broadcasts_between(chain_id, span_from, span_to)
            .await?;
        broadcasts.sort_by_key(|broadcast| broadcast.observed_at);

        let window = self.config.window_secs.max(1);
        let mut acc: Vec<LinkContribution> = Vec::new();
        let mut matched_pairs = 0usize;

        for broadcast in &broadcasts {
            let lo_time = broadcast.observed_at.saturating_sub(self.config.skew_secs);
            let hi_time = broadcast.observed_at + window;
            let lo = senders.partition_point(|(timestamp, _)| *timestamp < lo_time);
            let hi = senders.partition_point(|(timestamp, _)| *timestamp <= hi_time);
            for (timestamp, address) in &senders[lo..hi] {
                let delta = timestamp.saturating_sub(broadcast.observed_at);
                let weight = 1.0 - (delta.min(window) as f64 / window as f64);
                let contribution = (broadcast.confidence * weight).clamp(0.0, 1.0);
                let address_txs = totals.get(address).copied().unwrap_or(0);
                merge(
                    &mut acc,
                    broadcast.src_ip,
                    chain_id,
                    *address,
                    contribution,
                    address_txs,
                    *timestamp,
                );
                matched_pairs += 1;
            }
        }

        for contribution in &acc {
            self.store.apply_link(contribution).await?;
        }
        self.store.set_watermark(chain_id, up_to_block).await?;

        Ok(CorrelationRun {
            from_block,
            to_block: up_to_block,
            matched_pairs,
            links_touched: acc.len(),
        })
    }
}

fn merge(
    acc: &mut Vec<LinkContribution>,
    src_ip: IpAddr,
    chain_id: u64,
    address: EthAddress,
    contribution: f64,
    address_txs: u32,
    timestamp: u64,
) {
    if let Some(existing) = acc
        .iter_mut()
        .find(|entry| entry.src_ip == src_ip && entry.address == address)
    {
        existing.contribution += (1.0 - existing.contribution) * contribution;
        existing.hits += 1;
        existing.address_txs = address_txs;
        existing.first_seen = existing.first_seen.min(timestamp);
        existing.last_seen = existing.last_seen.max(timestamp);
    } else {
        acc.push(LinkContribution {
            src_ip,
            chain_id,
            address,
            contribution,
            hits: 1,
            address_txs,
            first_seen: timestamp,
            last_seen: timestamp,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::Mutex;

    use alloy_primitives::{B256, Bytes, U256};

    use domain::account::IpLinkId;
    use domain::eth::{EthReceipt, EthTx, MinedTx};

    use super::*;

    fn addr(byte: u8) -> EthAddress {
        EthAddress::from([byte; 20])
    }

    fn ip(byte: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, byte))
    }

    struct MockSource {
        rows: Vec<(u64, u64, EthAddress)>,
    }

    #[async_trait::async_trait]
    impl EthTxRepository for MockSource {
        async fn indexed_blocks(&self, lower: u64, upper: u64) -> Result<Vec<u64>, io::Error> {
            Ok(self
                .rows
                .iter()
                .map(|(block, _, _)| *block)
                .filter(|block| *block >= lower && *block <= upper)
                .collect())
        }

        async fn txs(&self, lower: u64, upper: u64) -> Result<Vec<MinedTx>, io::Error> {
            Ok(self
                .rows
                .iter()
                .filter(|(block, _, _)| *block >= lower && *block <= upper)
                .map(|(_, timestamp, from)| {
                    let tx = EthTx::builder()
                        .tx_hash(B256::ZERO)
                        .block_number(0)
                        .timestamp(*timestamp)
                        .amount(U256::ZERO)
                        .from(*from)
                        .data(Bytes::new())
                        .build();
                    MinedTx::new(tx, EthReceipt::new(true, None, vec![]))
                })
                .collect())
        }

        async fn save(&self, _lower: u64, _upper: u64, _txs: &[MinedTx]) -> Result<(), io::Error> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockStore {
        broadcasts: Vec<ObservedBroadcast>,
        links: Mutex<Vec<IpWalletLink>>,
        watermark: Mutex<Option<u64>>,
    }

    #[async_trait::async_trait]
    impl ObservationStore for MockStore {
        async fn insert(&self, _observation: &TlsObservation) -> Result<(), io::Error> {
            Ok(())
        }

        async fn broadcasts_between(
            &self,
            _chain_id: u64,
            from: u64,
            to: u64,
        ) -> Result<Vec<ObservedBroadcast>, io::Error> {
            Ok(self
                .broadcasts
                .iter()
                .copied()
                .filter(|broadcast| broadcast.observed_at >= from && broadcast.observed_at <= to)
                .collect())
        }

        async fn apply_link(&self, contribution: &LinkContribution) -> Result<(), io::Error> {
            let mut links = self.links.lock().unwrap();
            if let Some(link) = links
                .iter_mut()
                .find(|link| link.src_ip == contribution.src_ip && link.address == contribution.address)
            {
                link.confidence += (1.0 - link.confidence) * contribution.contribution;
                link.hits += contribution.hits;
                link.address_txs += contribution.address_txs;
                link.first_seen = link.first_seen.min(contribution.first_seen);
                link.last_seen = link.last_seen.max(contribution.last_seen);
            } else {
                links.push(IpWalletLink {
                    id: IpLinkId::new(),
                    src_ip: contribution.src_ip,
                    chain_id: contribution.chain_id,
                    address: contribution.address,
                    hits: contribution.hits,
                    address_txs: contribution.address_txs,
                    confidence: contribution.contribution,
                    first_seen: contribution.first_seen,
                    last_seen: contribution.last_seen,
                });
            }
            Ok(())
        }

        async fn links(&self, _chain_id: u64) -> Result<Vec<IpWalletLink>, io::Error> {
            Ok(self.links.lock().unwrap().clone())
        }

        async fn watermark(&self, _chain_id: u64) -> Result<Option<u64>, io::Error> {
            Ok(*self.watermark.lock().unwrap())
        }

        async fn set_watermark(&self, _chain_id: u64, block: u64) -> Result<(), io::Error> {
            *self.watermark.lock().unwrap() = Some(block);
            Ok(())
        }

        async fn reset(&self, _chain_id: u64) -> Result<(), io::Error> {
            self.links.lock().unwrap().clear();
            *self.watermark.lock().unwrap() = None;
            Ok(())
        }
    }

    fn link_for<'a>(links: &'a [IpWalletLink], who: IpAddr, what: EthAddress) -> Option<&'a IpWalletLink> {
        links.iter().find(|link| link.src_ip == who && link.address == what)
    }

    #[tokio::test]
    async fn repeated_cooccurrence_outweighs_single_and_respects_direction() {
        let broadcasts = vec![
            ObservedBroadcast { src_ip: ip(1), observed_at: 100, confidence: 0.9 },
            ObservedBroadcast { src_ip: ip(1), observed_at: 1000, confidence: 0.9 },
        ];
        let source = Arc::new(MockSource {
            rows: vec![
                (1, 110, addr(0xAA)),
                (1, 110, addr(0xBB)),
                (1, 90, addr(0xCC)),
                (2, 1008, addr(0xAA)),
            ],
        });
        let store = Arc::new(MockStore { broadcasts, ..MockStore::default() });
        let service = TlsCorrelationService::new(store.clone(), source, TlsCorrelationConfig::default());

        let run = service.correlate(1, None, 2).await.unwrap();
        assert_eq!(run.to_block, 2);

        let links = store.links(1).await.unwrap();
        let repeated = link_for(&links, ip(1), addr(0xAA)).expect("recurring wallet linked");
        let single = link_for(&links, ip(1), addr(0xBB)).expect("one-off wallet linked");

        assert_eq!(repeated.hits, 2);
        assert_eq!(single.hits, 1);
        assert!(repeated.confidence > single.confidence);
        assert!(link_for(&links, ip(1), addr(0xCC)).is_none());
    }

    #[tokio::test]
    async fn watermark_blocks_reprocessing() {
        let source = Arc::new(MockSource { rows: vec![(1, 110, addr(0xAA))] });
        let store = Arc::new(MockStore {
            broadcasts: vec![ObservedBroadcast { src_ip: ip(1), observed_at: 100, confidence: 0.9 }],
            ..MockStore::default()
        });
        let service = TlsCorrelationService::new(store.clone(), source, TlsCorrelationConfig::default());

        let first = service.correlate(1, None, 1).await.unwrap();
        assert_eq!(first.matched_pairs, 1);
        let again = service.correlate(1, None, 1).await.unwrap();
        assert_eq!(again.matched_pairs, 0);
        assert_eq!(store.links(1).await.unwrap()[0].hits, 1);
    }
}
