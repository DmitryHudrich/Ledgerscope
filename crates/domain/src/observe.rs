use std::net::IpAddr;

use crate::account::{IpLinkId, ObservationId};
use crate::eth::EthAddress;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservedIntent {
    Broadcast,
    Query,
}

impl ObservedIntent {
    pub fn as_str(self) -> &'static str {
        match self {
            ObservedIntent::Broadcast => "broadcast",
            ObservedIntent::Query => "query",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "broadcast" => Some(ObservedIntent::Broadcast),
            "query" => Some(ObservedIntent::Query),
            _ => None,
        }
    }

    pub fn is_broadcast(self) -> bool {
        matches!(self, ObservedIntent::Broadcast)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TlsObservation {
    pub id: ObservationId,
    pub src_ip: IpAddr,
    pub chain_id: u64,
    pub intent: ObservedIntent,
    pub confidence: f64,
    pub observed_at: u64,
}

impl TlsObservation {
    pub fn new(
        src_ip: IpAddr,
        chain_id: u64,
        intent: ObservedIntent,
        confidence: f64,
        observed_at: u64,
    ) -> Self {
        Self {
            id: ObservationId::new(),
            src_ip,
            chain_id,
            intent,
            confidence,
            observed_at,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IpWalletLink {
    pub id: IpLinkId,
    pub src_ip: IpAddr,
    pub chain_id: u64,
    pub address: EthAddress,
    pub hits: u32,
    pub address_txs: u32,
    pub confidence: f64,
    pub first_seen: u64,
    pub last_seen: u64,
}

impl IpWalletLink {
    pub fn score(&self) -> f64 {
        if self.address_txs == 0 {
            return 0.0;
        }
        (f64::from(self.hits) / f64::from(self.address_txs)).clamp(0.0, 1.0)
    }
}
