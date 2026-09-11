pub struct BtcTx {
    tx_hash: String,
    block_number: u64,
    timestamp: u64,
    inputs: Vec<BtcUtxo>,
    outputs: Vec<BtcUtxo>,
}

pub struct BtcUtxo {
    owner: String,
    amount: u128,
}

