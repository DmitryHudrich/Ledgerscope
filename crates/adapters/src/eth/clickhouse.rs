mod actor_row;
mod row;

use std::{collections::HashMap, io};

use application::eth::ports::{ActorRepository, EthTxIndex, EthTxRepository};
use domain::eth::{Actor, BlockBucket, BlockRange, EthAddress, IndexCoverage, MinedTx};
use reqwest::{Client, Url, header::CONTENT_LENGTH};
use serde::{Deserialize, Serialize};

use crate::eth::{
    chainlist::ETHEREUM,
    clickhouse::{actor_row::ActorRow, row::TxRow},
};

const TXS_TABLE: &str = "txs";
const BLOCKS_TABLE: &str = "indexed_blocks";
const ACTORS_TABLE: &str = "actors";

const TXS_COLUMNS: &str = "tx_hash, block_number, timestamp, amount, from_address, to_address, \
    data, succeeded, contract_address, log_addresses, log_topics, log_data";
const BLOCKS_COLUMNS: &str = "block_number";
const ACTORS_COLUMNS: &str = "address, kind, symbol, decimals";

const LEGACY_TABLES: [(&str, &str, &str); 3] = [
    ("eth_txs", TXS_TABLE, TXS_COLUMNS),
    ("eth_indexed_blocks", BLOCKS_TABLE, BLOCKS_COLUMNS),
    ("eth_actors", ACTORS_TABLE, ACTORS_COLUMNS),
];

const TXS_LATER_COLUMNS: [&str; 4] = [
    "asset_id String DEFAULT ''",
    "asset_symbol String DEFAULT ''",
    "asset_decimals UInt8 DEFAULT 0",
    "action LowCardinality(String) DEFAULT ''",
];

const TXS_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS txs (
    chain_id UInt64,
    tx_hash String,
    block_number UInt64,
    timestamp UInt64,
    amount String,
    from_address String,
    to_address Nullable(String),
    data String,
    succeeded UInt8,
    contract_address Nullable(String),
    log_addresses Array(String),
    log_topics Array(Array(String)),
    log_data Array(String),
    asset_id String DEFAULT '',
    asset_symbol String DEFAULT '',
    asset_decimals UInt8 DEFAULT 0,
    action LowCardinality(String) DEFAULT ''
) ENGINE = ReplacingMergeTree ORDER BY (chain_id, block_number, tx_hash)";

const BLOCKS_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS indexed_blocks (
    chain_id UInt64,
    block_number UInt64
) ENGINE = ReplacingMergeTree ORDER BY (chain_id, block_number)";

const ACTORS_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS actors (
    chain_id UInt64,
    address String,
    kind LowCardinality(String),
    symbol String,
    decimals UInt8
) ENGINE = ReplacingMergeTree ORDER BY (chain_id, address)";

#[derive(Clone)]
pub struct ClickhouseTxRepository {
    http_client: Client,
    url: Url,
    database: String,
    user: String,
    password: String,
    chain_id: u64,
}

#[derive(Serialize)]
struct OnChain<'a, T> {
    chain_id: u64,
    #[serde(flatten)]
    row: &'a T,
}

impl ClickhouseTxRepository {
    pub fn new(
        http_client: Client,
        url: Url,
        database: String,
        user: String,
        password: String,
    ) -> Self {
        Self {
            http_client,
            url,
            database,
            user,
            password,
            chain_id: ETHEREUM,
        }
    }

    pub fn for_chain(&self, chain_id: u64) -> Self {
        Self {
            chain_id,
            ..self.clone()
        }
    }

    pub async fn ensure_schema(&self) -> Result<(), io::Error> {
        self.send(
            None,
            &format!(
                "CREATE DATABASE IF NOT EXISTS {}",
                quote_identifier(&self.database)
            ),
            String::new(),
        )
        .await?;

        self.execute(TXS_SCHEMA, String::new()).await?;
        for column in TXS_LATER_COLUMNS {
            self.execute(
                &format!("ALTER TABLE {TXS_TABLE} ADD COLUMN IF NOT EXISTS {column}"),
                String::new(),
            )
            .await?;
        }
        self.execute(BLOCKS_SCHEMA, String::new()).await?;
        self.execute(ACTORS_SCHEMA, String::new()).await?;

        for (legacy, table, columns) in LEGACY_TABLES {
            self.adopt_legacy(legacy, table, columns).await?;
        }

        Ok(())
    }

    async fn adopt_legacy(
        &self,
        legacy: &str,
        table: &str,
        columns: &str,
    ) -> Result<(), io::Error> {
        let exists = self
            .execute(&format!("EXISTS TABLE {legacy}"), String::new())
            .await?;
        if exists.trim() != "1" {
            return Ok(());
        }

        let adopted: Vec<CountRow> = self
            .rows(&format!(
                "SELECT count() AS value FROM {table} WHERE chain_id = {ETHEREUM}"
            ))
            .await?;
        if adopted.first().is_some_and(|row| row.value > 0) {
            return Ok(());
        }

        tracing::info!("moving the single-chain {legacy} into {table} as chain {ETHEREUM}");
        self.execute(
            &format!(
                "INSERT INTO {table} (chain_id, {columns}) SELECT {ETHEREUM}, {columns} FROM {legacy}"
            ),
            String::new(),
        )
        .await?;

        Ok(())
    }

    async fn execute(&self, query: &str, body: String) -> Result<String, io::Error> {
        self.send(Some(&self.database), query, body).await
    }

    async fn send(
        &self,
        database: Option<&str>,
        query: &str,
        body: String,
    ) -> Result<String, io::Error> {
        let mut params = vec![
            ("query", query),
            ("output_format_json_quote_64bit_integers", "0"),
        ];

        if let Some(database) = database {
            params.push(("database", database));
        }

        let answer = self
            .http_client
            .post(self.url.clone())
            .query(&params)
            .header("X-ClickHouse-User", &self.user)
            .header("X-ClickHouse-Key", &self.password)
            .header(CONTENT_LENGTH, body.len())
            .body(body)
            .send()
            .await
            .map_err(|error| io::Error::other(format!("clickhouse is unreachable: {error}")))?;

        let status = answer.status();
        let text = answer
            .text()
            .await
            .map_err(|error| io::Error::other(format!("clickhouse answered badly: {error}")))?;

        if !status.is_success() {
            return Err(io::Error::other(format!(
                "clickhouse refused the query with {status}: {}",
                text.trim()
            )));
        }

        Ok(text)
    }

    fn on_chain<'a, T>(&self, row: &'a T) -> OnChain<'a, T> {
        OnChain {
            chain_id: self.chain_id,
            row,
        }
    }

    async fn rows<T: for<'de> Deserialize<'de>>(&self, query: &str) -> Result<Vec<T>, io::Error> {
        let text = self
            .execute(&format!("{query} FORMAT JSONEachRow"), String::new())
            .await?;

        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str(line).map_err(|error| {
                    io::Error::other(format!("clickhouse sent a odd row: {error}"))
                })
            })
            .collect()
    }
}

#[derive(Deserialize)]
struct BlockRow {
    block_number: u64,
}

#[derive(Deserialize)]
struct RangeRow {
    from_block: u64,
    to_block: u64,
}

#[derive(Deserialize)]
struct CountRow {
    value: u64,
}

#[derive(Deserialize)]
struct BucketRow {
    bucket: u64,
    value: u64,
}

#[async_trait::async_trait]
impl EthTxIndex for ClickhouseTxRepository {
    fn persistent(&self) -> bool {
        true
    }

    async fn coverage(&self, span: BlockRange) -> Result<IndexCoverage, io::Error> {
        let chain = self.chain_id;
        let (lower, highest) = (span.from_block(), span.to_block());

        let runs: Vec<RangeRow> = self
            .rows(&format!(
                "SELECT min(block_number) AS from_block, max(block_number) AS to_block FROM ( \
                     SELECT block_number, \
                            block_number - row_number() OVER (ORDER BY block_number) AS run \
                     FROM ( \
                         SELECT DISTINCT block_number FROM {BLOCKS_TABLE} \
                         WHERE chain_id = {chain} AND block_number BETWEEN {lower} AND {highest} \
                     ) \
                 ) GROUP BY run ORDER BY from_block"
            ))
            .await?;

        let counted: Vec<CountRow> = self
            .rows(&format!(
                "SELECT uniqExact(tx_hash) AS value FROM {TXS_TABLE} \
                 WHERE chain_id = {chain} AND block_number BETWEEN {lower} AND {highest}"
            ))
            .await?;

        Ok(IndexCoverage::new(
            runs.into_iter()
                .map(|row| BlockRange::new(row.from_block, row.to_block))
                .collect(),
            counted.first().map(|row| row.value).unwrap_or_default(),
        ))
    }

    async fn histogram(
        &self,
        span: BlockRange,
        buckets: u32,
    ) -> Result<Vec<BlockBucket>, io::Error> {
        let chain = self.chain_id;
        let buckets = u64::from(buckets.max(1));
        let (lower, highest) = (span.from_block(), span.to_block());
        let size = span.block_count().div_ceil(buckets).max(1);

        let blocks: Vec<BucketRow> = self
            .rows(&format!(
                "SELECT intDiv(block_number - {lower}, {size}) AS bucket, count() AS value FROM ( \
                     SELECT DISTINCT block_number FROM {BLOCKS_TABLE} \
                     WHERE chain_id = {chain} AND block_number BETWEEN {lower} AND {highest} \
                 ) GROUP BY bucket ORDER BY bucket"
            ))
            .await?;

        let txs: Vec<BucketRow> = self
            .rows(&format!(
                "SELECT intDiv(block_number - {lower}, {size}) AS bucket, \
                        uniqExact(tx_hash) AS value FROM {TXS_TABLE} \
                 WHERE chain_id = {chain} AND block_number BETWEEN {lower} AND {highest} \
                 GROUP BY bucket ORDER BY bucket"
            ))
            .await?;

        let indexed: HashMap<u64, u64> = blocks
            .into_iter()
            .map(|row| (row.bucket, row.value))
            .collect();
        let counted: HashMap<u64, u64> =
            txs.into_iter().map(|row| (row.bucket, row.value)).collect();

        Ok((0..span.block_count().div_ceil(size))
            .map(|bucket| {
                let from = lower + bucket * size;
                let to = (from + size - 1).min(highest);

                BlockBucket::new(
                    BlockRange::new(from, to),
                    indexed.get(&bucket).copied().unwrap_or_default(),
                    counted.get(&bucket).copied().unwrap_or_default(),
                )
            })
            .collect())
    }

    async fn txs_touching(
        &self,
        addresses: &[EthAddress],
        span: BlockRange,
    ) -> Result<Vec<MinedTx>, io::Error> {
        let chain = self.chain_id;
        if addresses.is_empty() {
            return Ok(Vec::new());
        }

        let (lower, highest) = (span.from_block(), span.to_block());
        let plain = quoted(addresses.iter().map(EthAddress::to_string));
        let words = quoted(
            addresses
                .iter()
                .map(|address| address.address().into_word().to_string()),
        );

        let rows: Vec<TxRow> = self
            .rows(&format!(
                "SELECT * FROM {TXS_TABLE} FINAL \
                 WHERE chain_id = {chain} AND block_number BETWEEN {lower} AND {highest} \
                 AND ( \
                     from_address IN ({plain}) \
                     OR to_address IN ({plain}) \
                     OR contract_address IN ({plain}) \
                     OR arrayExists(topics -> hasAny(topics, [{words}]), log_topics) \
                 ) ORDER BY block_number"
            ))
            .await?;

        rows.into_iter().map(TxRow::into_mined).collect()
    }
}

#[async_trait::async_trait]
impl ActorRepository for ClickhouseTxRepository {
    async fn actors(&self, addresses: &[EthAddress]) -> Result<Vec<Actor>, io::Error> {
        let chain = self.chain_id;
        if addresses.is_empty() {
            return Ok(Vec::new());
        }

        let wanted = quoted(addresses.iter().map(EthAddress::to_string));

        let rows: Vec<ActorRow> = self
            .rows(&format!(
                "SELECT * FROM {ACTORS_TABLE} FINAL \
                 WHERE chain_id = {chain} AND address IN ({wanted})"
            ))
            .await?;

        rows.into_iter().map(ActorRow::into_actor).collect()
    }

    async fn remember(&self, actors: &[Actor]) -> Result<(), io::Error> {
        let mut body = String::new();

        for actor in actors {
            let Some(row) = ActorRow::from_actor(actor) else {
                continue;
            };

            let line = serde_json::to_string(&self.on_chain(&row)).map_err(|error| {
                io::Error::other(format!("failed to encode {}: {error}", row.address))
            })?;

            body.push_str(&line);
            body.push('\n');
        }

        if body.is_empty() {
            return Ok(());
        }

        self.execute(
            &format!("INSERT INTO {ACTORS_TABLE} FORMAT JSONEachRow"),
            body,
        )
        .await?;

        Ok(())
    }
}

fn quoted(values: impl Iterator<Item = String>) -> String {
    values
        .map(|value| format!("'{}'", value.replace('\'', "\\'")))
        .collect::<Vec<_>>()
        .join(",")
}

#[async_trait::async_trait]
impl EthTxRepository for ClickhouseTxRepository {
    async fn indexed_blocks(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> Result<Vec<u64>, io::Error> {
        let chain = self.chain_id;
        let rows: Vec<BlockRow> = self
            .rows(&format!(
                "SELECT block_number FROM {BLOCKS_TABLE} \
                 WHERE chain_id = {chain} AND block_number BETWEEN {lower_block} AND {highest_block} \
                 GROUP BY block_number ORDER BY block_number"
            ))
            .await?;

        Ok(rows.into_iter().map(|row| row.block_number).collect())
    }

    async fn txs(&self, lower_block: u64, highest_block: u64) -> Result<Vec<MinedTx>, io::Error> {
        let chain = self.chain_id;
        let rows: Vec<TxRow> = self
            .rows(&format!(
                "SELECT * FROM {TXS_TABLE} FINAL \
                 WHERE chain_id = {chain} AND block_number BETWEEN {lower_block} AND {highest_block} \
                 AND block_number IN ( \
                     SELECT block_number FROM {BLOCKS_TABLE} \
                     WHERE chain_id = {chain} AND block_number BETWEEN {lower_block} AND {highest_block} \
                 ) ORDER BY block_number"
            ))
            .await?;

        rows.into_iter().map(TxRow::into_mined).collect()
    }

    async fn save(
        &self,
        lower_block: u64,
        highest_block: u64,
        txs: &[MinedTx],
    ) -> Result<(), io::Error> {
        if !txs.is_empty() {
            let mut body = String::new();

            for tx in txs {
                let row = TxRow::from_mined(tx);
                let line = serde_json::to_string(&self.on_chain(&row)).map_err(|error| {
                    io::Error::other(format!("failed to encode {}: {error}", row.tx_hash))
                })?;

                body.push_str(&line);
                body.push('\n');
            }

            self.execute(&format!("INSERT INTO {TXS_TABLE} FORMAT JSONEachRow"), body)
                .await?;
        }

        let blocks: String = (lower_block..=highest_block)
            .map(|block| format!("({}, {block})", self.chain_id))
            .collect::<Vec<_>>()
            .join(",");

        self.execute(
            &format!("INSERT INTO {BLOCKS_TABLE} (chain_id, block_number) VALUES {blocks}"),
            String::new(),
        )
        .await?;

        Ok(())
    }
}

fn quote_identifier(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}
