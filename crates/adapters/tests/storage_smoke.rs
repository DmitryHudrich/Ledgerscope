use std::sync::Arc;

use alloy_primitives::{B256, Bytes, TxHash, U256};
use application::eth::{
    StoringEthTxSource,
    ports::{EthTxCache, EthTxRepository, EthTxSource},
};
use domain::eth::{EthAddress, EthLog, EthReceipt, EthTx, MinedTx};
use futures::StreamExt;
use reqwest::{Client, Url};

use adapters::eth::{ClickhouseTxRepository, RedisTxCache};

const CLICKHOUSE_URL: &str = "LEDGERSCOPE_TEST_CLICKHOUSE_URL";
const REDIS_URL: &str = "LEDGERSCOPE_TEST_REDIS_URL";

fn run_id() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

fn mined(block_number: u64, nonce: u8) -> MinedTx {
    let tx = EthTx::builder()
        .tx_hash(TxHash::with_last_byte(nonce))
        .block_number(block_number)
        .timestamp(block_number * 12)
        .amount(U256::from(nonce) * U256::from(10_u64).pow(U256::from(18)))
        .from(EthAddress::from([nonce; 20]))
        .to(EthAddress::from([nonce.wrapping_add(1); 20]))
        .data(Bytes::from_static(&[0xca, 0xfe]))
        .build();

    let receipt = EthReceipt::new(
        nonce.is_multiple_of(2),
        Some(EthAddress::from([nonce.wrapping_add(2); 20])),
        vec![EthLog::new(
            EthAddress::from([nonce.wrapping_add(3); 20]),
            vec![B256::with_last_byte(nonce)],
            Bytes::from_static(&[0x01]),
        )],
    );

    MinedTx::new(tx, receipt)
}

async fn repository(database: &str) -> Option<ClickhouseTxRepository> {
    let url = std::env::var(CLICKHOUSE_URL)
        .ok()
        .filter(|u| !u.is_empty())?;

    let repository = ClickhouseTxRepository::new(
        Client::new(),
        Url::parse(&url).unwrap(),
        format!("{database}_{}", std::process::id()),
        std::env::var("LEDGERSCOPE_TEST_CLICKHOUSE_USER").unwrap_or_else(|_| "default".to_owned()),
        std::env::var("LEDGERSCOPE_TEST_CLICKHOUSE_PASSWORD").unwrap_or_default(),
    );

    repository.ensure_schema().await.unwrap();

    Some(repository)
}

#[tokio::test]
async fn a_saved_range_reads_back_whole() {
    let Some(repository) = repository("ledgerscope_smoke_whole").await else {
        return;
    };

    repository
        .save(100, 102, &[mined(100, 1), mined(101, 2)])
        .await
        .unwrap();

    assert_eq!(
        repository.indexed_blocks(100, 102).await.unwrap(),
        vec![100, 101, 102]
    );
    assert!(
        repository
            .indexed_blocks(200, 202)
            .await
            .unwrap()
            .is_empty()
    );

    let mut stored = repository.txs(100, 102).await.unwrap();
    stored.sort_by_key(|tx| tx.tx().block_number());

    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].tx().tx_hash(), mined(100, 1).tx().tx_hash());
    assert_eq!(stored[0].tx().amount(), mined(100, 1).tx().amount());
    assert_eq!(stored[0].tx().to(), mined(100, 1).tx().to());
    assert!(!stored[0].receipt().succeeded());
    assert!(stored[1].receipt().succeeded());
    assert_eq!(
        stored[1].receipt().logs()[0].topics(),
        mined(101, 2).receipt().logs()[0].topics()
    );
}

#[tokio::test]
async fn an_empty_range_is_still_remembered() {
    let Some(repository) = repository("ledgerscope_smoke_empty").await else {
        return;
    };

    repository.save(500, 501, &[]).await.unwrap();

    assert_eq!(
        repository.indexed_blocks(500, 501).await.unwrap(),
        vec![500, 501]
    );
    assert!(repository.txs(500, 501).await.unwrap().is_empty());
}

#[tokio::test]
async fn saving_the_same_block_twice_keeps_one_tx() {
    let Some(repository) = repository("ledgerscope_smoke_twice").await else {
        return;
    };

    repository.save(700, 700, &[mined(700, 5)]).await.unwrap();
    repository.save(700, 700, &[mined(700, 5)]).await.unwrap();

    assert_eq!(
        repository.indexed_blocks(700, 700).await.unwrap(),
        vec![700]
    );
    assert_eq!(repository.txs(700, 700).await.unwrap().len(), 1);
}

#[tokio::test]
async fn redis_remembers_indexed_blocks() {
    let Some(url) = std::env::var(REDIS_URL).ok().filter(|u| !u.is_empty()) else {
        return;
    };

    let cache = RedisTxCache::connect_with_key(&url, &format!("ledgerscope:test:{}", run_id()))
        .await
        .unwrap();

    cache.remember_indexed(&[900, 901, 905]).await.unwrap();

    assert_eq!(
        cache.indexed_blocks(900, 902).await.unwrap(),
        vec![900, 901]
    );
    assert_eq!(
        cache.indexed_blocks(900, 905).await.unwrap(),
        vec![900, 901, 905]
    );
    assert!(cache.indexed_blocks(1_000, 1_100).await.unwrap().is_empty());
}

struct CountingUpstream {
    calls: std::sync::Mutex<Vec<(u64, u64)>>,
}

#[async_trait::async_trait]
impl EthTxSource for CountingUpstream {
    async fn txs(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> application::BoxStream<'_, Result<MinedTx, std::io::Error>> {
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

#[tokio::test]
async fn the_second_request_is_served_without_the_upstream() {
    let Some(repository) = repository("ledgerscope_smoke_layer").await else {
        return;
    };

    let cache = match std::env::var(REDIS_URL).ok().filter(|u| !u.is_empty()) {
        Some(url) => Some(Arc::new(
            RedisTxCache::connect_with_key(&url, &format!("ledgerscope:test:{}", run_id()))
                .await
                .unwrap(),
        ) as Arc<dyn EthTxCache>),
        None => None,
    };

    let upstream = Arc::new(CountingUpstream {
        calls: std::sync::Mutex::new(Vec::new()),
    });

    let source = StoringEthTxSource::new(upstream.clone(), Arc::new(repository), cache);

    let first: Vec<u64> = source
        .txs(1_000, 1_002)
        .await
        .filter_map(|item| async move { item.ok() })
        .map(|tx| tx.tx().block_number())
        .collect()
        .await;

    assert_eq!(first, vec![1_000, 1_001, 1_002]);
    assert_eq!(upstream.calls.lock().unwrap().clone(), vec![(1_000, 1_002)]);

    let mut second: Vec<u64> = source
        .txs(1_000, 1_002)
        .await
        .filter_map(|item| async move { item.ok() })
        .map(|tx| tx.tx().block_number())
        .collect()
        .await;
    second.sort_unstable();

    assert_eq!(second, vec![1_000, 1_001, 1_002]);
    assert_eq!(upstream.calls.lock().unwrap().len(), 1);
}
