pub struct EthTx {
    tx_hash: String,
    block_number: u64,
    timestamp: u64,

    amount: u128,

    from: String,
    to: Option<String>,
    data: Option<String>,
}

