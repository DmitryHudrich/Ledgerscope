#![cfg(feature = "aws-public")]

use adapters::eth::aws_public::{AwsPublicConfig, AwsPublicTxSource};
use application::eth::ports::EthTxSource;
use domain::eth::BlockRange;
use futures::StreamExt;
use reqwest::{Client, Url};

const SWITCH: &str = "LEDGERSCOPE_TEST_AWS_PUBLIC";
const BUCKET: &str = "https://aws-public-blockchain.s3.us-east-2.amazonaws.com/";
const FIRST_BLOCK_OF_2016: u64 = 778_483;

fn source(cache: &std::path::Path) -> Option<AwsPublicTxSource> {
    std::env::var(SWITCH).ok().filter(|on| on == "1")?;
    Some(AwsPublicTxSource::new(
        Client::new(),
        AwsPublicConfig {
            base_url: Url::parse(BUCKET).unwrap(),
            prefix: "v1.0/eth".to_owned(),
            cache_dir: cache.to_path_buf(),
            keep_days: 1,
            window_blocks: 1_000,
        },
    ))
}

#[tokio::test]
async fn an_early_day_reads_back_with_its_logs() {
    let cache = std::env::temp_dir().join(format!("ledgerscope-aws-{}", std::process::id()));
    let Some(source) = source(&cache) else {
        return;
    };

    let span = BlockRange::new(FIRST_BLOCK_OF_2016, FIRST_BLOCK_OF_2016 + 199);
    let windows = source.windows(span).await.unwrap();
    assert_eq!(windows.first().unwrap().from_block(), FIRST_BLOCK_OF_2016);
    assert_eq!(
        windows.last().unwrap().to_block(),
        FIRST_BLOCK_OF_2016 + 199
    );
    assert!(source.available_until().await.unwrap() > 20_000_000);

    let txs: Vec<_> = source
        .txs(span.from_block(), span.to_block())
        .await
        .map(|tx| tx.unwrap())
        .collect()
        .await;

    assert!(!txs.is_empty());
    assert!(txs.iter().all(|tx| span.contains(tx.tx().block_number())));
    assert!(
        txs.windows(2)
            .all(|pair| pair[0].tx().block_number() <= pair[1].tx().block_number())
    );
    assert!(txs.iter().any(|tx| !tx.receipt().logs().is_empty()));
    assert!(txs.iter().any(|tx| !tx.tx().amount().is_zero()));

    let _ = std::fs::remove_dir_all(cache);
}
