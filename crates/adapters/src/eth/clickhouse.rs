mod row;

use std::io;

use application::eth::ports::EthTxRepository;
use domain::eth::MinedTx;
use reqwest::{Client, Url};
use serde::Deserialize;

use crate::eth::clickhouse::row::TxRow;

const TXS_TABLE: &str = "eth_txs";
const BLOCKS_TABLE: &str = "eth_indexed_blocks";

const TXS_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS eth_txs (
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
    log_data Array(String)
) ENGINE = ReplacingMergeTree ORDER BY (block_number, tx_hash)";

const BLOCKS_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS eth_indexed_blocks (
    block_number UInt64
) ENGINE = ReplacingMergeTree ORDER BY block_number";

pub struct ClickhouseTxRepository {
    http_client: Client,
    url: Url,
    database: String,
    user: String,
    password: String,
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
        self.execute(BLOCKS_SCHEMA, String::new()).await?;

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

#[async_trait::async_trait]
impl EthTxRepository for ClickhouseTxRepository {
    async fn indexed_blocks(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> Result<Vec<u64>, io::Error> {
        let rows: Vec<BlockRow> = self
            .rows(&format!(
                "SELECT block_number FROM {BLOCKS_TABLE} \
                 WHERE block_number BETWEEN {lower_block} AND {highest_block} \
                 GROUP BY block_number ORDER BY block_number"
            ))
            .await?;

        Ok(rows.into_iter().map(|row| row.block_number).collect())
    }

    async fn txs(&self, lower_block: u64, highest_block: u64) -> Result<Vec<MinedTx>, io::Error> {
        let rows: Vec<TxRow> = self
            .rows(&format!(
                "SELECT * FROM {TXS_TABLE} FINAL \
                 WHERE block_number BETWEEN {lower_block} AND {highest_block} \
                 AND block_number IN ( \
                     SELECT block_number FROM {BLOCKS_TABLE} \
                     WHERE block_number BETWEEN {lower_block} AND {highest_block} \
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
                let line = serde_json::to_string(&row).map_err(|error| {
                    io::Error::other(format!("failed to encode {}: {error}", row.tx_hash))
                })?;

                body.push_str(&line);
                body.push('\n');
            }

            self.execute(&format!("INSERT INTO {TXS_TABLE} FORMAT JSONEachRow"), body)
                .await?;
        }

        let blocks: String = (lower_block..=highest_block)
            .map(|block| format!("({block})"))
            .collect::<Vec<_>>()
            .join(",");

        self.execute(
            &format!("INSERT INTO {BLOCKS_TABLE} (block_number) VALUES {blocks}"),
            String::new(),
        )
        .await?;

        Ok(())
    }
}

fn quote_identifier(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}
