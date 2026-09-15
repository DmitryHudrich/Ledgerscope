use std::io;

use serde_json::Value;

use alloy_primitives::{Bytes, TxHash};
use application::{
    BoxStream,
    eth::ports::{EthRpcSource, EthTxSource},
};
use domain::eth::{BlockRef, EthAddress, EthReceipt, EthTx};

use crate::eth::rpc::parse::{hex_to_bytes, parse_block, parse_receipt};

pub struct RpcTxSource {
    http_client: reqwest::Client,
    rpc_url: String,
}

impl RpcTxSource {
    pub fn new(http_client: reqwest::Client, rpc_url: String) -> Self {
        Self {
            http_client,
            rpc_url,
        }
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, io::Error> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });

        let mut response: Value = self
            .http_client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(io::Error::other)?
            .error_for_status()
            .map_err(io::Error::other)?
            .json()
            .await
            .map_err(io::Error::other)?;

        if let Some(error) = response.get("error").filter(|error| !error.is_null()) {
            return Err(io::Error::other(format!("{method} failed: {error}")));
        }

        Ok(response["result"].take())
    }
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
    ) -> BoxStream<'_, Result<EthTx, io::Error>> {
        let result =
            get_txs_range(&self.http_client, &self.rpc_url, lower_block, highest_block).await;

        Box::pin(async_stream::stream! {
            match result {
                Ok(txs) => {
                    for tx in txs {
                        yield Ok(tx);
                    }
                }
                Err(e) => {
                    yield Err(io::Error::other(e));
                }
            }
        })
    }
}

async fn get_txs_range(
    client: &reqwest::Client,
    rpc_url: &str,
    from_block: u64,
    to_block: u64,
) -> Result<Vec<EthTx>, reqwest::Error> {
    let batch: Vec<Value> = (from_block..=to_block)
        .enumerate()
        .map(|(id, block_num)| {
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "eth_getBlockByNumber",
                "params": [format!("0x{:x}", block_num), true]
            })
        })
        .collect();

    let response: Vec<Value> = client
        .post(rpc_url)
        .json(&batch)
        .send()
        .await?
        .json()
        .await?;

    let mut all_txs = Vec::new();
    for entry in response {
        if let Some(block) = entry.get("result") {
            all_txs.extend(parse_block(block));
        }
    }

    Ok(all_txs)
}
