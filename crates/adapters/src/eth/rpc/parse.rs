use serde_json::Value;

use application::eth::ports::{EthLog, EthReceipt};
use domain::eth::{EthAddress, EthTx};

pub fn hex_to_u64(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)
}

pub fn hex_to_u128(s: &str) -> u128 {
    u128::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)
}

pub fn hex_to_bytes(s: &str) -> Vec<u8> {
    let s = s.trim_start_matches("0x");
    hex::decode(s).unwrap_or_default()
}

pub fn parse_receipt(receipt: &Value) -> EthReceipt {
    let succeeded = receipt["status"]
        .as_str()
        .map(|status| hex_to_u64(status) == 1)
        .unwrap_or(true);

    let contract_address = receipt["contractAddress"]
        .as_str()
        .and_then(|s| s.parse::<EthAddress>().ok());

    let logs = receipt["logs"]
        .as_array()
        .map(|logs| logs.iter().filter_map(parse_log).collect())
        .unwrap_or_default();

    EthReceipt::new(succeeded, contract_address, logs)
}

fn parse_log(log: &Value) -> Option<EthLog> {
    let address = log["address"].as_str()?.parse::<EthAddress>().ok()?;

    let topics = log["topics"]
        .as_array()?
        .iter()
        .filter_map(|topic| topic.as_str())
        .filter_map(parse_topic)
        .collect();

    let data = hex_to_bytes(log["data"].as_str().unwrap_or("0x"));

    Some(EthLog::new(address, topics, data))
}

fn parse_topic(topic: &str) -> Option<[u8; 32]> {
    <[u8; 32]>::try_from(hex_to_bytes(topic).as_slice()).ok()
}

pub fn parse_block(block: &Value) -> Vec<EthTx> {
    let timestamp = hex_to_u64(block["timestamp"].as_str().unwrap_or("0x0"));

    let txs = match block["transactions"].as_array() {
        Some(txs) => txs,
        None => return Vec::new(),
    };

    txs.iter()
        .filter_map(|tx| {
            let tx_hash = tx["hash"].as_str()?.to_string();
            let block_number = hex_to_u64(tx["blockNumber"].as_str()?);
            let amount = hex_to_u128(tx["value"].as_str()?);
            let from = tx["from"].as_str()?.parse::<EthAddress>().ok()?;
            let to = tx["to"].as_str().and_then(|s| s.parse::<EthAddress>().ok());
            let data = hex_to_bytes(tx["input"].as_str()?);

            Some(
                EthTx::builder()
                    .tx_hash(tx_hash)
                    .block_number(block_number)
                    .timestamp(timestamp)
                    .amount(amount)
                    .from(from)
                    .maybe_to(to)
                    .data(data)
                    .build(),
            )
        })
        .collect()
}
