use std::{
    collections::HashMap,
    io,
    str::FromStr,
    sync::{Arc, Mutex},
};

use alloy_primitives::{B256, Bytes, TxHash, U256};
use application::{
    BoxStream,
    eth::ports::{EthRpcSource, EthTxSource},
};
use domain::eth::{BlockRef, EthAddress, EthLog, EthReceipt, EthTx, IssuedAsset, MinedTx};
use futures::{StreamExt, TryStreamExt};
use reqwest::Url;
use serde_json::{Value, json};

use crate::eth::rpc::RpcPool;

pub const TRON_MAINNET: u64 = 728_126_428;
pub const TRON_SHASTA: u64 = 2_494_104_990;
pub const TRON_NILE: u64 = 3_448_148_188;
pub const TRON_TESTNETS: [u64; 2] = [TRON_NILE, TRON_SHASTA];

const BLOCKS_IN_FLIGHT: usize = 4;
const SUCCESS: &str = "SUCCESS";
const REVERT_PREFIX: &str = "REVERT";
const ZERO_OWNER: &str = "410000000000000000000000000000000000000000";

const MAINNET_NODES: &[&str] = &[
    "https://api.trongrid.io",
    "https://tron-rpc.publicnode.com",
    "https://tron.publicnode.com",
    "https://api.tronstack.io",
    "https://tron.api.pocket.network",
    "https://tron-mainnet.gateway.tatum.io",
];
const NILE_NODES: &[&str] = &["https://nile.trongrid.io"];
const SHASTA_NODES: &[&str] = &["https://api.shasta.trongrid.io"];

const COUNTERPARTY_FIELDS: [&str; 5] = [
    "to_address",
    "receiver_address",
    "account_address",
    "contract_address",
    "exchange_id",
];
const AMOUNT_FIELDS: [&str; 5] = [
    "amount",
    "balance",
    "frozen_balance",
    "unfreeze_balance",
    "quant",
];

pub fn is_tron(chain_id: u64) -> bool {
    matches!(chain_id, TRON_MAINNET | TRON_SHASTA | TRON_NILE)
}

pub fn chain_id_of_genesis(block_id: &str) -> Option<u64> {
    let hex = block_id.trim().trim_start_matches("0x");
    if hex.len() != 64 {
        return None;
    }
    u64::from_str_radix(hex.get(56..)?, 16).ok()
}

pub fn public_nodes(chain_id: u64) -> Vec<Url> {
    let nodes = match chain_id {
        TRON_MAINNET => MAINNET_NODES,
        TRON_NILE => NILE_NODES,
        TRON_SHASTA => SHASTA_NODES,
        _ => &[],
    };

    nodes
        .iter()
        .filter_map(|url| Url::parse(url).ok())
        .collect()
}

pub struct TronTxSource {
    pool: Arc<RpcPool>,
    assets: Mutex<HashMap<String, IssuedAsset>>,
}

impl TronTxSource {
    pub fn new(pool: Arc<RpcPool>) -> Self {
        Self {
            pool,
            assets: Mutex::new(HashMap::new()),
        }
    }

    async fn block(&self, number: u64) -> io::Result<Vec<MinedTx>> {
        let body = json!({ "num": number, "visible": false });
        let (block, infos) = futures::future::try_join(
            self.pool
                .post_json("wallet/getblockbynum", &body, |answer| {
                    answer.get("blockID").is_some()
                }),
            self.pool.post_json(
                "wallet/gettransactioninfobyblocknum",
                &body,
                |answer| {
                    answer.is_array() || answer.as_object().is_some_and(|map| map.is_empty())
                },
            ),
        )
        .await?;

        let infos: HashMap<&str, &Value> = infos
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|info| Some((info.get("id")?.as_str()?, info)))
            .collect();

        let header = &block["block_header"]["raw_data"];
        let timestamp = header["timestamp"].as_u64().unwrap_or_default() / 1000;

        let mut mined = Vec::new();
        for tx in block["transactions"].as_array().into_iter().flatten() {
            let id = tx["txID"].as_str().unwrap_or_default();
            match self
                .mined(tx, infos.get(id).copied(), number, timestamp)
                .await
            {
                Ok(found) => mined.push(found),
                Err(error) => {
                    tracing::warn!("tron tx {id} in block {number} is unreadable: {error}")
                }
            }
        }

        Ok(mined)
    }

    async fn mined(
        &self,
        tx: &Value,
        info: Option<&Value>,
        block_number: u64,
        timestamp: u64,
    ) -> io::Result<MinedTx> {
        let contract = &tx["raw_data"]["contract"][0];
        let kind = contract["type"].as_str().unwrap_or_default();
        let value = &contract["parameter"]["value"];

        let builder = EthTx::builder()
            .tx_hash(hash(tx["txID"].as_str().unwrap_or_default())?)
            .block_number(block_number)
            .timestamp(timestamp)
            .from(address(&value["owner_address"])?);

        let tx_built = match kind {
            "TransferContract" => builder
                .to(address(&value["to_address"])?)
                .amount(number(&value["amount"]))
                .data(Bytes::new())
                .build(),
            "TransferAssetContract" => builder
                .to(address(&value["to_address"])?)
                .amount(number(&value["amount"]))
                .data(Bytes::new())
                .asset(self.asset(&asset_id(&value["asset_name"])).await)
                .build(),
            "TriggerSmartContract" => builder
                .to(address(&value["contract_address"])?)
                .amount(number(&value["call_value"]))
                .data(bytes(&value["data"])?)
                .build(),
            "CreateSmartContract" => builder.amount(U256::ZERO).data(Bytes::new()).build(),
            other => builder
                .maybe_to(
                    COUNTERPARTY_FIELDS
                        .iter()
                        .find_map(|field| address(&value[*field]).ok()),
                )
                .amount(
                    AMOUNT_FIELDS
                        .iter()
                        .map(|field| number(&value[*field]))
                        .find(|amount| !amount.is_zero())
                        .unwrap_or_default(),
                )
                .data(Bytes::new())
                .action(other.trim_end_matches("Contract").to_owned())
                .build(),
        };

        let succeeded = tx["ret"][0]["contractRet"]
            .as_str()
            .or_else(|| info.and_then(|info| info["receipt"]["result"].as_str()))
            .is_none_or(|result| result == SUCCESS);

        let deployed = kind == "CreateSmartContract";
        Ok(MinedTx::new(tx_built, receipt(info, succeeded, deployed)?))
    }

    async fn asset(&self, id: &str) -> IssuedAsset {
        if let Some(known) = self
            .assets
            .lock()
            .expect("the lock is never poisoned")
            .get(id)
        {
            return known.clone();
        }

        let found = self
            .pool
            .post_json(
                "wallet/getassetissuebyid",
                &json!({ "value": id }),
                Value::is_object,
            )
            .await;

        let asset = match found {
            Ok(issue) => IssuedAsset::new(
                id.to_owned(),
                text(&issue["abbr"])
                    .or_else(|| text(&issue["name"]))
                    .unwrap_or_else(|| id.to_owned()),
                issue["precision"]
                    .as_u64()
                    .and_then(|precision| u8::try_from(precision).ok())
                    .unwrap_or_default(),
            ),
            Err(error) => {
                tracing::warn!("trc-10 {id} keeps its id for a name: {error}");
                return IssuedAsset::new(id.to_owned(), id.to_owned(), 0);
            }
        };

        self.assets
            .lock()
            .expect("the lock is never poisoned")
            .insert(id.to_owned(), asset.clone());
        asset
    }
}

fn receipt(info: Option<&Value>, succeeded: bool, deployed: bool) -> io::Result<EthReceipt> {
    let Some(info) = info else {
        return Ok(EthReceipt::new(succeeded, None, Vec::new()));
    };

    let logs = info["log"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|log| {
            Ok(EthLog::new(
                address(&log["address"])?,
                log["topics"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(word)
                    .collect::<io::Result<_>>()?,
                bytes(&log["data"])?,
            ))
        })
        .collect::<io::Result<Vec<_>>>()?;

    Ok(EthReceipt::new(
        succeeded,
        deployed
            .then(|| address(&info["contract_address"]).ok())
            .flatten(),
        logs,
    ))
}

fn address(value: &Value) -> io::Result<EthAddress> {
    let raw = value
        .as_str()
        .ok_or_else(|| io::Error::other("no address"))?;
    EthAddress::from_tron_hex(raw).map_err(io::Error::other)
}

fn hash(raw: &str) -> io::Result<TxHash> {
    TxHash::from_str(raw).map_err(|error| io::Error::other(format!("tx id {raw}: {error}")))
}

fn word(value: &Value) -> io::Result<B256> {
    let raw = value.as_str().unwrap_or_default();
    B256::from_str(raw).map_err(|error| io::Error::other(format!("topic {raw}: {error}")))
}

fn bytes(value: &Value) -> io::Result<Bytes> {
    match value.as_str() {
        None | Some("") => Ok(Bytes::new()),
        Some(raw) => {
            Bytes::from_str(raw).map_err(|error| io::Error::other(format!("hex {raw}: {error}")))
        }
    }
}

fn number(value: &Value) -> U256 {
    value
        .as_u64()
        .map(U256::from)
        .or_else(|| value.as_str().and_then(|raw| U256::from_str(raw).ok()))
        .unwrap_or_default()
}

fn text(value: &Value) -> Option<String> {
    let raw = value.as_str()?;
    let decoded = hex::decode(raw).ok()?;
    String::from_utf8(decoded)
        .ok()
        .filter(|text| !text.trim().is_empty())
}

fn asset_id(value: &Value) -> String {
    text(value).unwrap_or_else(|| value.as_str().unwrap_or_default().to_owned())
}

fn head(answer: &Value) -> io::Result<u64> {
    answer["block_header"]["raw_data"]["number"]
        .as_u64()
        .ok_or_else(|| io::Error::other("getblock named no block"))
}

#[async_trait::async_trait]
impl EthRpcSource for TronTxSource {
    async fn head_block(&self) -> Result<u64, io::Error> {
        let answer = self
            .pool
            .post_json("wallet/getblock", &json!({ "detail": false }), |answer| {
                answer.get("block_header").is_some()
            })
            .await?;

        head(&answer)
    }

    async fn call(
        &self,
        to: &EthAddress,
        data: &[u8],
        _block: BlockRef,
    ) -> Result<Bytes, io::Error> {
        let answer = self
            .pool
            .post_json(
                "wallet/triggerconstantcontract",
                &json!({
                    "owner_address": ZERO_OWNER,
                    "contract_address": to.tron_hex(),
                    "data": hex::encode(data),
                    "visible": false,
                }),
                |answer| answer.get("result").is_some(),
            )
            .await?;

        let result = &answer["result"];
        let message = text(&result["message"]).unwrap_or_default();
        if result["result"].as_bool() != Some(true) || message.starts_with(REVERT_PREFIX) {
            let code = result["code"].as_str().unwrap_or(REVERT_PREFIX);
            return Err(io::Error::other(format!(
                "triggerconstantcontract failed: execution reverted ({code}: {message})"
            )));
        }

        bytes(&answer["constant_result"][0])
    }

    async fn code(&self, address: &EthAddress, _block: BlockRef) -> Result<Bytes, io::Error> {
        let answer = self
            .pool
            .post_json(
                "wallet/getcontract",
                &json!({ "value": address.tron_hex(), "visible": false }),
                Value::is_object,
            )
            .await?;

        if answer.get("contract_address").is_none() {
            return Ok(Bytes::new());
        }

        let code = bytes(&answer["bytecode"])?;
        Ok(if code.is_empty() {
            Bytes::from_static(&[0])
        } else {
            code
        })
    }

    async fn receipt(&self, tx_hash: &TxHash) -> Result<Option<EthReceipt>, io::Error> {
        let answer = self
            .pool
            .post_json(
                "wallet/gettransactioninfobyid",
                &json!({ "value": hex::encode(tx_hash) }),
                Value::is_object,
            )
            .await?;

        if answer.get("id").is_none() {
            return Ok(None);
        }

        let succeeded = answer["receipt"]["result"]
            .as_str()
            .is_none_or(|result| result == SUCCESS);
        receipt(Some(&answer), succeeded, false).map(Some)
    }
}

#[async_trait::async_trait]
impl EthTxSource for TronTxSource {
    async fn txs(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> BoxStream<'_, Result<MinedTx, io::Error>> {
        let blocks: Result<Vec<Vec<MinedTx>>, io::Error> =
            futures::stream::iter(lower_block..=highest_block)
                .map(|number| self.block(number))
                .buffered(BLOCKS_IN_FLIGHT)
                .try_collect()
                .await;

        Box::pin(async_stream::stream! {
            match blocks {
                Ok(blocks) => {
                    for tx in blocks.into_iter().flatten() {
                        yield Ok(tx);
                    }
                }
                Err(error) => yield Err(error),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trc10_names_come_out_of_hex() {
        assert_eq!(asset_id(&json!("31303035313933")), "1005193");
        assert_eq!(text(&json!("6761732e6269")).as_deref(), Some("gas.bi"));
    }

    #[test]
    fn a_tron_log_reads_like_an_evm_log() {
        let info = json!({
            "id": "9f448fef95d362d2b0e8c4d79b1b51a59bda01c70d478a3d1e1cf8b1f5251244",
            "contract_address": "41a614f803b6fd780986a42c78ec9c7f77e6ded13c",
            "log": [{
                "address": "a614f803b6fd780986a42c78ec9c7f77e6ded13c",
                "topics": [
                    "ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
                    "000000000000000000000000c686c48436aec3a1dfce4ac5a4526c39366985ba",
                    "0000000000000000000000006853b7e828125061940940680b41e16a401f02b4"
                ],
                "data": "0000000000000000000000000000000000000000000000000000000008f0d180"
            }]
        });

        let receipt = receipt(Some(&info), true, false).unwrap();

        assert_eq!(receipt.logs().len(), 1);
        assert_eq!(
            receipt.logs()[0].address().tron_base58(),
            "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t"
        );
        assert_eq!(receipt.logs()[0].topics().len(), 3);
    }

    #[test]
    fn only_tron_chains_are_tron() {
        assert!(is_tron(TRON_MAINNET));
        assert_eq!(
            chain_id_of_genesis("00000000000000001ebf88508a03865c71d452e25f4d51194196a1d22b6653dc"),
            Some(TRON_MAINNET)
        );
        assert_eq!(chain_id_of_genesis("2b6653dc"), None);
        assert!(!is_tron(1));
        assert_eq!(public_nodes(TRON_MAINNET).len(), MAINNET_NODES.len());
    }
}
