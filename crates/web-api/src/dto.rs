use bon::Builder;
use serde::Serialize;

use application::eth::GraphSnapshot;
use domain::eth::{EthAddress, EthTx};

#[derive(Serialize)]
pub struct GraphResponse {
    nodes: Vec<String>,
    edges: Vec<TxResponse>,
}

impl GraphResponse {
    pub fn new(nodes: Vec<String>, edges: Vec<TxResponse>) -> Self {
        Self { nodes, edges }
    }
}

#[derive(Serialize, Builder)]
pub struct TxResponse {
    tx_hash: String,
    block_number: u64,
    timestamp: u64,
    amount: String,
    from: String,
    to: Option<String>,
    data: String,
}

impl From<GraphSnapshot> for GraphResponse {
    fn from(snapshot: GraphSnapshot) -> Self {
        Self::new(
            snapshot.nodes().iter().map(EthAddress::to_string).collect(),
            snapshot.edges().iter().map(TxResponse::from).collect(),
        )
    }
}

impl From<&EthTx> for TxResponse {
    fn from(tx: &EthTx) -> Self {
        TxResponse::builder()
            .tx_hash(tx.tx_hash().to_string())
            .block_number(tx.block_number())
            .timestamp(tx.timestamp())
            .amount(tx.amount().to_string())
            .from(tx.from().to_string())
            .maybe_to(tx.to().map(EthAddress::to_string))
            .data(format!("0x{}", hex::encode(tx.data())))
            .build()
    }
}
