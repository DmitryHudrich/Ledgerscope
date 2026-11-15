use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use application::observe::CorrelationRun;
use domain::observe::IpWalletLink;

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ObservationRequest {
    #[schema(example = "203.0.113.9")]
    pub src_ip: String,
    #[schema(example = 3448148188u64)]
    pub chain_id: u64,
    #[schema(example = "broadcast")]
    pub intent: String,
    #[schema(example = 0.9)]
    pub confidence: f64,
    #[schema(example = 1700000000u64)]
    pub observed_at: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct IpWalletLinkResponse {
    pub src_ip: String,
    pub chain_id: u64,
    pub address: String,
    pub hits: u32,
    pub address_txs: u32,
    pub score: f64,
    pub confidence: f64,
    pub first_seen: u64,
    pub last_seen: u64,
}

impl From<&IpWalletLink> for IpWalletLinkResponse {
    fn from(link: &IpWalletLink) -> Self {
        Self {
            src_ip: link.src_ip.to_string(),
            chain_id: link.chain_id,
            address: format!("0x{}", link.address.hex()),
            hits: link.hits,
            address_txs: link.address_txs,
            score: link.score(),
            confidence: link.confidence,
            first_seen: link.first_seen,
            last_seen: link.last_seen,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CorrelateResponse {
    pub from_block: u64,
    pub to_block: u64,
    pub matched_pairs: usize,
    pub links_touched: usize,
}

impl From<CorrelationRun> for CorrelateResponse {
    fn from(run: CorrelationRun) -> Self {
        Self {
            from_block: run.from_block,
            to_block: run.to_block,
            matched_pairs: run.matched_pairs,
            links_touched: run.links_touched,
        }
    }
}
