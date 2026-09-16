use std::{
    collections::HashMap,
    io,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::{StatusCode, header::RETRY_AFTER};
use serde_json::Value;

use alloy_primitives::{Bytes, TxHash};
use application::{
    BoxStream,
    eth::ports::{EthRpcSource, EthTxSource},
};
use domain::eth::{BlockRef, EthAddress, EthReceipt, MinedTx};

use crate::eth::rpc::parse::{hex_to_bytes, parse_block, parse_block_receipts, parse_receipt};

mod rate_limit;

pub const DEFAULT_RATE_LIMIT_RPS: u32 = 15;

const MAX_ATTEMPTS: u32 = 5;
const BASE_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(8);
const RATE_LIMITED_CODE: i64 = -32005;
const REQUEST_LIMIT_CODE: i64 = -32007;

enum RpcFailure {
    Retryable {
        reason: String,
        after: Option<Duration>,
    },
    Fatal(io::Error),
}

pub struct RpcTxSource {
    http_client: reqwest::Client,
    rpc_url: String,
    rate_limiter: rate_limit::RateLimiter,
}

impl RpcTxSource {
    pub fn new(http_client: reqwest::Client, rpc_url: String) -> Self {
        Self {
            http_client,
            rpc_url,
            rate_limiter: rate_limit::RateLimiter::new(DEFAULT_RATE_LIMIT_RPS),
        }
    }

    pub fn with_rate_limit(mut self, rps: u32) -> Self {
        self.rate_limiter = rate_limit::RateLimiter::new(rps);
        self
    }

    async fn send(&self, method: &str, body: &Value, requests: usize) -> Result<Value, io::Error> {
        let mut attempt = 1;

        loop {
            self.rate_limiter.acquire(requests).await;

            match self.send_once(body).await {
                Ok(value) => return Ok(value),
                Err(RpcFailure::Fatal(error)) => return Err(error),
                Err(RpcFailure::Retryable { reason, after }) => {
                    if attempt >= MAX_ATTEMPTS {
                        return Err(io::Error::other(format!(
                            "{method} gave up after {attempt} attempts: {reason}"
                        )));
                    }

                    let delay = after.unwrap_or_else(|| backoff(attempt));
                    tracing::warn!(
                        "{method} attempt {attempt}/{MAX_ATTEMPTS} failed ({reason}), pausing every caller for {delay:?}"
                    );
                    self.rate_limiter.throttle(delay).await;
                    attempt += 1;
                }
            }
        }
    }

    async fn send_once(&self, body: &Value) -> Result<Value, RpcFailure> {
        let response = self
            .http_client
            .post(&self.rpc_url)
            .json(body)
            .send()
            .await
            .map_err(|error| {
                if error.is_connect() || error.is_timeout() || error.is_request() {
                    RpcFailure::Retryable {
                        reason: error.to_string(),
                        after: None,
                    }
                } else {
                    RpcFailure::Fatal(io::Error::other(error))
                }
            })?;

        let status = response.status();

        if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            return Err(RpcFailure::Retryable {
                reason: format!("http {status}"),
                after: retry_after(&response),
            });
        }

        if !status.is_success() {
            return Err(RpcFailure::Fatal(io::Error::other(format!(
                "http {status}"
            ))));
        }

        let value: Value = response
            .json()
            .await
            .map_err(|error| RpcFailure::Fatal(io::Error::other(error)))?;

        match rate_limit_reason(&value) {
            Some(reason) => Err(RpcFailure::Retryable {
                reason,
                after: None,
            }),
            None => Ok(value),
        }
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, io::Error> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let mut response = self.send(method, &body, 1).await?;

        if let Some(error) = response.get("error").filter(|error| !error.is_null()) {
            return Err(io::Error::other(format!("{method} failed: {error}")));
        }

        Ok(response["result"].take())
    }

    async fn batch(
        &self,
        method: &str,
        from_block: u64,
        to_block: u64,
        params: impl Fn(u64) -> Value,
    ) -> Result<Vec<Value>, io::Error> {
        let blocks: Vec<u64> = (from_block..=to_block).collect();
        let mut results: Vec<Option<Value>> = vec![None; blocks.len()];
        let mut pending: Vec<usize> = (0..blocks.len()).collect();
        let mut attempt = 1;

        while !pending.is_empty() {
            let mut limited = Vec::new();

            for chunk in pending.chunks(self.rate_limiter.burst()) {
                let body: Vec<Value> = chunk
                    .iter()
                    .map(|&id| {
                        serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "method": method,
                            "params": params(blocks[id]),
                        })
                    })
                    .collect();

                let response = self.send(method, &Value::from(body), chunk.len()).await?;

                let entries = response
                    .as_array()
                    .ok_or_else(|| io::Error::other(format!("{method} returned no batch")))?;

                for entry in entries {
                    let id = entry["id"].as_u64().unwrap_or(u64::MAX) as usize;
                    let Some(slot) = results.get_mut(id) else {
                        return Err(io::Error::other(format!(
                            "{method} answered with id {id}, which was never asked"
                        )));
                    };

                    match entry.get("result").filter(|result| !result.is_null()) {
                        Some(result) => *slot = Some(result.clone()),
                        None => {
                            let error = &entry["error"];
                            if !rate_limited(error) {
                                return Err(io::Error::other(format!(
                                    "{method} for block {}: {error}",
                                    blocks[id]
                                )));
                            }
                            limited.push(id);
                        }
                    }
                }
            }

            if limited.is_empty() {
                break;
            }

            if attempt >= MAX_ATTEMPTS {
                return Err(io::Error::other(format!(
                    "{method} gave up on {} of {} blocks after {attempt} attempts",
                    limited.len(),
                    blocks.len()
                )));
            }

            let delay = backoff(attempt);
            tracing::warn!(
                "{method}: {} of {} blocks hit the request limit, retrying them in {delay:?}",
                limited.len(),
                blocks.len()
            );
            self.rate_limiter.throttle(delay).await;

            pending = limited;
            attempt += 1;
        }

        Ok(results.into_iter().flatten().collect())
    }

    async fn mined_txs(&self, from_block: u64, to_block: u64) -> Result<Vec<MinedTx>, io::Error> {
        let blocks = self.batch(
            "eth_getBlockByNumber",
            from_block,
            to_block,
            |block_number| serde_json::json!([format!("0x{block_number:x}"), true]),
        );

        let receipts = self.batch(
            "eth_getBlockReceipts",
            from_block,
            to_block,
            |block_number| serde_json::json!([format!("0x{block_number:x}")]),
        );

        let (blocks, receipts) = futures::future::try_join(blocks, receipts).await?;

        let receipts: HashMap<TxHash, EthReceipt> =
            receipts.iter().flat_map(parse_block_receipts).collect();

        let mut mined = Vec::new();
        for block in &blocks {
            for tx in parse_block(block) {
                match receipts.get(tx.tx_hash()) {
                    Some(receipt) => mined.push(MinedTx::new(tx, receipt.clone())),
                    None => tracing::warn!("no receipt for {}, skipping it", tx.tx_hash()),
                }
            }
        }

        Ok(mined)
    }
}

fn backoff(attempt: u32) -> Duration {
    let exponential = BASE_BACKOFF * 2u32.saturating_pow(attempt - 1);
    let jitter = Duration::from_millis(u64::from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| since.subsec_millis() % 100)
            .unwrap_or(0),
    ));

    exponential.min(MAX_BACKOFF) + jitter
}

fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    let seconds = response
        .headers()
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;

    Some(Duration::from_secs(seconds).min(MAX_BACKOFF))
}

fn rate_limit_reason(value: &Value) -> Option<String> {
    let error = value.get("error").filter(|error| !error.is_null())?;
    rate_limited(error).then(|| error.to_string())
}

fn rate_limited(error: &Value) -> bool {
    let code = error.get("code").and_then(Value::as_i64);

    if code == Some(RATE_LIMITED_CODE) || code == Some(REQUEST_LIMIT_CODE) {
        return true;
    }

    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_lowercase();

    message.contains("rate limit")
        || message.contains("request limit")
        || message.contains("limit reached")
        || message.contains("too many requests")
}

fn block_param(block: BlockRef) -> Value {
    match block {
        BlockRef::Latest => Value::from("latest"),
        BlockRef::Number(number) => Value::from(format!("0x{number:x}")),
    }
}

fn hex_data(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

#[async_trait::async_trait]
impl EthRpcSource for RpcTxSource {
    async fn call(
        &self,
        to: &EthAddress,
        data: &[u8],
        block: BlockRef,
    ) -> Result<Bytes, io::Error> {
        let params = serde_json::json!([
            {
                "to": to.to_string(),
                "data": hex_data(data),
            },
            block_param(block),
        ]);

        let result = self.request("eth_call", params).await?;

        match result.as_str() {
            Some(encoded) => Ok(hex_to_bytes(encoded)),
            None => Err(io::Error::other("eth_call returned no data")),
        }
    }

    async fn code(&self, address: &EthAddress, block: BlockRef) -> Result<Bytes, io::Error> {
        let params = serde_json::json!([address.to_string(), block_param(block)]);

        let result = self.request("eth_getCode", params).await?;

        match result.as_str() {
            Some(encoded) => Ok(hex_to_bytes(encoded)),
            None => Err(io::Error::other("eth_getCode returned no data")),
        }
    }

    async fn receipt(&self, tx_hash: &TxHash) -> Result<Option<EthReceipt>, io::Error> {
        let params = serde_json::json!([tx_hash.to_string()]);

        let result = self.request("eth_getTransactionReceipt", params).await?;

        if result.is_null() {
            return Ok(None);
        }

        Ok(Some(parse_receipt(&result)))
    }
}

#[async_trait::async_trait]
impl EthTxSource for RpcTxSource {
    async fn txs(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> BoxStream<'_, Result<MinedTx, io::Error>> {
        let result = self.mined_txs(lower_block, highest_block).await;

        Box::pin(async_stream::stream! {
            match result {
                Ok(mined) => {
                    for tx in mined {
                        yield Ok(tx);
                    }
                }
                Err(e) => {
                    yield Err(e);
                }
            }
        })
    }
}
