pub mod btc;
pub mod eth;

pub enum BlockchainTxType {
    Bitcoin(btc::BtcTx),
    Etherium(eth::EthTx),
}
