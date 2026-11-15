use serde::Serialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use utoipa::ToSchema;

use adapters::eth::{PoolTraffic, StorageUsage, TableUsage};

#[derive(Serialize, ToSchema)]
pub struct UsageResponse {
    pub since: String,
    pub storage: Option<StorageResponse>,
    pub traffic: TrafficResponse,
}

#[derive(Serialize, ToSchema)]
pub struct StorageResponse {
    pub bytes_on_disk: u64,
    pub uncompressed_bytes: u64,
    pub tables: Vec<TableResponse>,
    pub chains: Vec<ChainStorageResponse>,
}

#[derive(Serialize, ToSchema)]
pub struct TableResponse {
    pub table: String,
    pub rows: u64,
    pub bytes_on_disk: u64,
    pub uncompressed_bytes: u64,
}

#[derive(Serialize, ToSchema)]
pub struct ChainStorageResponse {
    pub chain_id: u64,
    pub name: Option<String>,
    pub txs: u64,
    pub estimated_bytes: u64,
}

#[derive(Serialize, ToSchema)]
pub struct TrafficResponse {
    pub calls: u64,
    pub sent_bytes: u64,
    pub received_bytes: u64,
    pub chains: Vec<ChainTrafficResponse>,
}

#[derive(Serialize, ToSchema)]
pub struct ChainTrafficResponse {
    pub chain_id: u64,
    pub name: String,
    pub calls: u64,
    pub sent_bytes: u64,
    pub received_bytes: u64,
}

impl UsageResponse {
    pub fn new(
        since: OffsetDateTime,
        storage: Option<StorageResponse>,
        traffic: Vec<ChainTrafficResponse>,
    ) -> Self {
        Self {
            since: since.format(&Rfc3339).unwrap_or_default(),
            storage,
            traffic: TrafficResponse::new(traffic),
        }
    }
}

impl StorageResponse {
    pub fn new(usage: StorageUsage, name: impl Fn(u64) -> Option<String>) -> Self {
        let txs_bytes = usage
            .tables
            .iter()
            .find(|table| table.table == "txs")
            .map_or(0, |table| table.bytes_on_disk);
        let total_txs: u64 = usage.chains.iter().map(|chain| chain.txs).sum();

        Self {
            bytes_on_disk: usage.tables.iter().map(|table| table.bytes_on_disk).sum(),
            uncompressed_bytes: usage
                .tables
                .iter()
                .map(|table| table.uncompressed_bytes)
                .sum(),
            chains: usage
                .chains
                .iter()
                .map(|chain| ChainStorageResponse {
                    chain_id: chain.chain_id,
                    name: name(chain.chain_id),
                    txs: chain.txs,
                    estimated_bytes: share(txs_bytes, chain.txs, total_txs),
                })
                .collect(),
            tables: usage.tables.into_iter().map(TableResponse::from).collect(),
        }
    }
}

impl From<TableUsage> for TableResponse {
    fn from(table: TableUsage) -> Self {
        Self {
            table: table.table,
            rows: table.rows,
            bytes_on_disk: table.bytes_on_disk,
            uncompressed_bytes: table.uncompressed_bytes,
        }
    }
}

impl ChainTrafficResponse {
    pub fn new(chain_id: u64, name: String, traffic: PoolTraffic) -> Self {
        Self {
            chain_id,
            name,
            calls: traffic.calls,
            sent_bytes: traffic.sent_bytes,
            received_bytes: traffic.received_bytes,
        }
    }
}

impl TrafficResponse {
    fn new(mut chains: Vec<ChainTrafficResponse>) -> Self {
        chains.retain(|chain| chain.calls > 0);
        chains.sort_by_key(|chain| std::cmp::Reverse(chain.sent_bytes + chain.received_bytes));
        Self {
            calls: chains.iter().map(|chain| chain.calls).sum(),
            sent_bytes: chains.iter().map(|chain| chain.sent_bytes).sum(),
            received_bytes: chains.iter().map(|chain| chain.received_bytes).sum(),
            chains,
        }
    }
}

fn share(bytes: u64, part: u64, whole: u64) -> u64 {
    if whole == 0 {
        return 0;
    }
    (u128::from(bytes) * u128::from(part) / u128::from(whole)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapters::eth::ChainTxCount;

    fn table(name: &str, bytes: u64) -> TableUsage {
        TableUsage {
            table: name.to_owned(),
            rows: 1,
            bytes_on_disk: bytes,
            uncompressed_bytes: bytes * 3,
        }
    }

    #[test]
    fn splits_the_txs_table_by_the_chains_share_of_rows() {
        let usage = StorageUsage {
            tables: vec![table("txs", 1_000), table("actors", 10)],
            chains: vec![
                ChainTxCount {
                    chain_id: 1,
                    txs: 3,
                },
                ChainTxCount {
                    chain_id: 42161,
                    txs: 1,
                },
            ],
        };

        let storage = StorageResponse::new(usage, |id| (id == 1).then(|| "Ethereum".to_owned()));

        assert_eq!(storage.bytes_on_disk, 1_010);
        assert_eq!(storage.uncompressed_bytes, 3_030);
        assert_eq!(storage.chains[0].estimated_bytes, 750);
        assert_eq!(storage.chains[0].name.as_deref(), Some("Ethereum"));
        assert_eq!(storage.chains[1].estimated_bytes, 250);
        assert_eq!(storage.chains[1].name, None);
    }

    #[test]
    fn traffic_sums_only_the_chains_that_were_asked() {
        let traffic = TrafficResponse::new(vec![
            ChainTrafficResponse::new(
                1,
                "Ethereum".into(),
                PoolTraffic {
                    calls: 2,
                    sent_bytes: 100,
                    received_bytes: 900,
                },
            ),
            ChainTrafficResponse::new(10, "Optimism".into(), PoolTraffic::default()),
            ChainTrafficResponse::new(
                56,
                "BNB".into(),
                PoolTraffic {
                    calls: 1,
                    sent_bytes: 50,
                    received_bytes: 5_000,
                },
            ),
        ]);

        assert_eq!(traffic.chains.len(), 2);
        assert_eq!(traffic.chains[0].chain_id, 56);
        assert_eq!(
            (traffic.calls, traffic.sent_bytes, traffic.received_bytes),
            (3, 150, 5_900)
        );
    }
}
