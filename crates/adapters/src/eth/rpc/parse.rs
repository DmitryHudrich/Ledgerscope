use serde_json::Value;

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
