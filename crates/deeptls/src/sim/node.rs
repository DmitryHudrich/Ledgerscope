use std::collections::HashMap;

use serde_json::{Value, json};

use crate::sim::rng::Rng;

const BASE58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
pub const TRANSFER_TOPIC: &str =
    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

pub fn hex_raw(rng: &mut Rng, bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rng.fill(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hex(rng: &mut Rng, bytes: usize) -> String {
    format!("0x{}", hex_raw(rng, bytes))
}

pub fn quantity(value: u64) -> String {
    format!("{value:#x}")
}

pub fn tron_address(rng: &mut Rng) -> String {
    let body: String = (0..33)
        .map(|_| BASE58[rng.below(BASE58.len() as u64) as usize] as char)
        .collect();
    format!("T{body}")
}

struct Pending {
    polls_left: u64,
    contract: bool,
}

pub struct TestNode {
    block: u64,
    pending: HashMap<String, Pending>,
}

impl TestNode {
    pub fn new(rng: &mut Rng) -> Self {
        Self {
            block: 6_000_000 + rng.below(1_000_000),
            pending: HashMap::new(),
        }
    }

    pub fn handle(&mut self, path: &str, request: &Value, rng: &mut Rng) -> (Value, f64) {
        self.block += rng.below(2);
        let (body, ms) = match (path, request) {
            ("/", Value::Array(batch)) => {
                let replies: Vec<_> = batch.iter().map(|r| self.eth(r, rng)).collect();
                let ms = replies.iter().map(|(_, ms)| ms).sum();
                (
                    Value::Array(replies.into_iter().map(|(v, _)| v).collect()),
                    ms,
                )
            }
            ("/", single) => self.eth(single, rng),
            (path, body) => self.tron(path, body, rng),
        };
        (body, ms / 1000.0)
    }

    fn eth(&mut self, request: &Value, rng: &mut Rng) -> (Value, f64) {
        let params = &request["params"];
        let (result, ms) = match request["method"].as_str().unwrap_or_default() {
            "eth_chainId" => (json!("0xaa36a7"), 1.0),
            "eth_blockNumber" => (json!(quantity(self.block)), 1.0),
            "eth_getTransactionCount" => (json!(quantity(rng.below(500))), 3.0),
            "eth_maxPriorityFeePerGas" | "eth_gasPrice" => (
                json!(quantity(1_000_000_000 + rng.below(2_000_000_000))),
                2.0,
            ),
            "eth_getBalance" => (json!(quantity(rng.next_u64() >> 4)), 4.0),
            "eth_call" => (json!(hex(rng, 32)), 8.0),
            "eth_estimateGas" => {
                let gas = if params[0]["data"].is_string() {
                    45_000 + rng.below(20_000)
                } else {
                    21_000
                };
                (json!(quantity(gas)), 12.0)
            }
            "eth_getBlockByNumber" => (self.eth_block(rng), 15.0),
            "eth_sendRawTransaction" => {
                let contract = params[0].as_str().map_or(0, str::len) > 300;
                let hash = hex(rng, 32);
                self.pending.insert(
                    hash.clone(),
                    Pending {
                        polls_left: rng.below(4),
                        contract,
                    },
                );
                (json!(hash), 30.0)
            }
            "eth_getTransactionReceipt" => (
                self.eth_receipt(params[0].as_str().unwrap_or_default(), rng),
                5.0,
            ),
            "eth_getLogs" => {
                let n = 5 + rng.below(80);
                let logs: Vec<_> = (0..n).map(|_| self.eth_log(&hex(rng, 32), rng)).collect();
                (json!(logs), 40.0 + 3.0 * n as f64)
            }
            other => {
                let error = json!({"code": -32601, "message": format!("the method {other} does not exist/is not available")});
                return (
                    json!({"jsonrpc": "2.0", "id": request["id"], "error": error}),
                    1.0,
                );
            }
        };
        (
            json!({"jsonrpc": "2.0", "id": request["id"], "result": result}),
            ms,
        )
    }

    fn eth_block(&self, rng: &mut Rng) -> Value {
        let txs: Vec<_> = (0..30 + rng.below(220)).map(|_| hex(rng, 32)).collect();
        json!({
            "number": quantity(self.block), "hash": hex(rng, 32), "parentHash": hex(rng, 32),
            "miner": hex(rng, 20), "stateRoot": hex(rng, 32), "transactionsRoot": hex(rng, 32),
            "receiptsRoot": hex(rng, 32), "logsBloom": hex(rng, 256), "difficulty": "0x0",
            "gasLimit": "0x1c9c380", "gasUsed": quantity(rng.below(30_000_000)),
            "baseFeePerGas": quantity(rng.below(50_000_000_000)), "timestamp": quantity(1_790_000_000 + self.block),
            "extraData": hex(rng, 12), "mixHash": hex(rng, 32), "nonce": "0x0000000000000000",
            "size": quantity(rng.below(200_000)), "withdrawalsRoot": hex(rng, 32), "uncles": [], "transactions": txs,
        })
    }

    fn eth_log(&self, tx_hash: &str, rng: &mut Rng) -> Value {
        json!({
            "address": hex(rng, 20),
            "topics": [TRANSFER_TOPIC, format!("0x{:0>64}", hex_raw(rng, 20)), format!("0x{:0>64}", hex_raw(rng, 20))],
            "data": hex(rng, 32), "blockNumber": quantity(self.block - rng.below(1000)), "blockHash": hex(rng, 32),
            "transactionHash": tx_hash, "transactionIndex": quantity(rng.below(200)),
            "logIndex": quantity(rng.below(500)), "removed": false,
        })
    }

    fn eth_receipt(&mut self, hash: &str, rng: &mut Rng) -> Value {
        let Some(contract) = self.poll(hash) else {
            return Value::Null;
        };
        let logs = if contract {
            vec![self.eth_log(hash, rng)]
        } else {
            vec![]
        };
        json!({
            "transactionHash": hash, "transactionIndex": quantity(rng.below(200)), "blockHash": hex(rng, 32),
            "blockNumber": quantity(self.block), "from": hex(rng, 20), "to": hex(rng, 20),
            "cumulativeGasUsed": quantity(rng.below(30_000_000)), "gasUsed": quantity(21_000 + rng.below(40_000)),
            "effectiveGasPrice": quantity(rng.below(60_000_000_000)), "contractAddress": null,
            "logs": logs, "logsBloom": hex(rng, 256), "status": "0x1", "type": "0x2",
        })
    }

    fn poll(&mut self, id: &str) -> Option<bool> {
        let pending = self.pending.get_mut(id)?;
        if pending.polls_left > 0 {
            pending.polls_left -= 1;
            return None;
        }
        self.pending.remove(id).map(|p| p.contract)
    }

    fn tron(&mut self, path: &str, request: &Value, rng: &mut Rng) -> (Value, f64) {
        match path {
            "/wallet/getnowblock" => (self.tron_block_header(rng), 3.0),
            "/wallet/getaccount" => {
                let assets: Vec<_> = (0..rng.below(6))
                    .map(|i| json!({"key": format!("100{i:04}"), "value": rng.below(1 << 40)}))
                    .collect();
                let account = json!({
                    "address": request["address"], "balance": rng.below(50_000_000_000),
                    "create_time": 1_700_000_000_000u64 + rng.below(1 << 36), "latest_opration_time": 1_790_000_000_000u64,
                    "free_net_usage": rng.below(600), "latest_consume_free_time": 1_790_000_000_000u64,
                    "net_window_size": 28800, "net_window_optimized": true,
                    "account_resource": {"energy_window_size": 28800, "energy_window_optimized": true},
                    "owner_permission": {"permission_name": "owner", "threshold": 1, "keys": [{"address": request["address"], "weight": 1}]},
                    "active_permission": [{"type": "Active", "id": 2, "permission_name": "active", "threshold": 1,
                        "operations": hex_raw(rng, 32), "keys": [{"address": request["address"], "weight": 1}]}],
                    "frozenV2": [{}, {"type": "ENERGY"}, {"type": "TRON_POWER"}], "assetV2": assets,
                });
                (account, 6.0)
            }
            "/wallet/getaccountresource" => {
                let resources = json!({
                    "freeNetLimit": 600, "NetLimit": rng.below(5000), "TotalNetLimit": 43_200_000_000u64,
                    "TotalNetWeight": 26_000_000_000u64 + rng.below(1 << 30), "EnergyLimit": rng.below(100_000),
                    "TotalEnergyLimit": 180_000_000_000u64, "TotalEnergyWeight": 19_000_000_000u64 + rng.below(1 << 30),
                    "tronPowerLimit": rng.below(10_000),
                });
                (resources, 4.0)
            }
            "/wallet/createtransaction" => {
                let value = json!({"amount": request["amount"], "owner_address": request["owner_address"], "to_address": request["to_address"]});
                (self.tron_tx("TransferContract", value, 106, rng), 10.0)
            }
            "/wallet/triggersmartcontract" | "/wallet/triggerconstantcontract" => {
                let selector = if request["function_selector"] == "balanceOf(address)" {
                    "70a08231"
                } else {
                    "a9059cbb"
                };
                let data = format!(
                    "{selector}{}",
                    request["parameter"].as_str().unwrap_or_default()
                );
                let value = json!({"data": data, "owner_address": request["owner_address"], "contract_address": request["contract_address"]});
                let mut reply = json!({"result": {"result": true}, "transaction": self.tron_tx("TriggerSmartContract", value, 172, rng)});
                if path.ends_with("constantcontract") {
                    reply["constant_result"] = json!([hex_raw(rng, 32)]);
                    reply["energy_used"] = json!(rng.below(1000));
                }
                (reply, 25.0)
            }
            "/wallet/broadcasttransaction" => {
                let txid = request["txID"].as_str().unwrap_or_default().to_owned();
                let contract = request["raw_data"]["contract"][0]["type"] == "TriggerSmartContract";
                self.pending.insert(
                    txid.clone(),
                    Pending {
                        polls_left: rng.below(3),
                        contract,
                    },
                );
                (json!({"result": true, "txid": txid}), 40.0)
            }
            "/wallet/gettransactioninfobyid" => (
                self.tron_info(request["value"].as_str().unwrap_or_default(), rng),
                5.0,
            ),
            _ => (json!({"Error": format!("{path} not found")}), 1.0),
        }
    }

    fn tron_block_header(&self, rng: &mut Rng) -> Value {
        json!({
            "blockID": hex_raw(rng, 32),
            "block_header": {
                "raw_data": {"number": self.block, "txTrieRoot": hex_raw(rng, 32), "witness_address": tron_address(rng),
                    "parentHash": hex_raw(rng, 32), "version": 31, "timestamp": 1_790_000_000_000u64 + self.block * 3000},
                "witness_signature": hex_raw(rng, 65),
            },
        })
    }

    fn tron_tx(&self, kind: &str, value: Value, raw_len: usize, rng: &mut Rng) -> Value {
        let mut raw_data = json!({
            "contract": [{"parameter": {"value": value, "type_url": format!("type.googleapis.com/protocol.{kind}")}, "type": kind}],
            "ref_block_bytes": hex_raw(rng, 2), "ref_block_hash": hex_raw(rng, 8),
            "expiration": 1_790_000_060_000u64 + self.block, "timestamp": 1_790_000_000_000u64 + self.block,
        });
        if kind == "TriggerSmartContract" {
            raw_data["fee_limit"] = json!(100_000_000);
        }
        json!({"visible": true, "txID": hex_raw(rng, 32), "raw_data": raw_data, "raw_data_hex": hex_raw(rng, raw_len)})
    }

    fn tron_info(&mut self, txid: &str, rng: &mut Rng) -> Value {
        let Some(contract) = self.poll(txid) else {
            return json!({});
        };
        let mut info = json!({
            "id": txid, "blockNumber": self.block, "blockTimeStamp": 1_790_000_000_000u64 + self.block * 3000,
            "contractResult": [""], "receipt": {"net_usage": 260 + rng.below(80)},
        });
        if contract {
            info["fee"] = json!(rng.below(30_000_000));
            info["contract_address"] = json!(tron_address(rng));
            info["contractResult"] = json!([hex_raw(rng, 32)]);
            info["receipt"] = json!({"energy_usage_total": 29_000 + rng.below(36_000), "energy_fee": rng.below(30_000_000),
                "net_usage": 340 + rng.below(30), "result": "SUCCESS"});
            info["log"] = json!([{"address": hex_raw(rng, 20), "data": hex_raw(rng, 32),
                "topics": [&TRANSFER_TOPIC[2..], format!("{:0>64}", hex_raw(rng, 20)), format!("{:0>64}", hex_raw(rng, 20))]}]);
        }
        info
    }
}
