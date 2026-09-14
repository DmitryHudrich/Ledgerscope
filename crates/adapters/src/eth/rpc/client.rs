use std::io;

use serde_json::Value;

use application::{BoxStream, eth::ports::EthTxSource};
use domain::eth::EthTx;

use crate::eth::rpc::parse::parse_block;

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
