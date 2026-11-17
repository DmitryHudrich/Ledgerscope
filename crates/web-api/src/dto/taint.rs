use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use application::eth::{
    TaintGraph,
    taint::{Asset, Holding, TaintedMove},
};
use domain::eth::{BlockRange, EthAddress};

use crate::dto::{ExtraLabels, GraphRequest, GraphResponse, TagSourceResponse};

#[derive(Debug, Deserialize, ToSchema)]
pub struct TaintRequest {
    #[serde(flatten)]
    pub graph: GraphRequest,

    #[serde(default)]
    pub min_share_bps: Option<u64>,

    #[serde(default)]
    pub stops: Vec<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Native,
    Token,
    Issued,
}

#[derive(Serialize, ToSchema)]
pub struct AssetResponse {
    kind: AssetKind,
    id: Option<String>,
}

impl From<&Asset> for AssetResponse {
    fn from(asset: &Asset) -> Self {
        match asset {
            Asset::Native => Self {
                kind: AssetKind::Native,
                id: None,
            },
            Asset::Token(token) => Self {
                kind: AssetKind::Token,
                id: Some(token.to_string()),
            },
            Asset::Issued(id) => Self {
                kind: AssetKind::Issued,
                id: Some(id.clone()),
            },
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct TaintFlowResponse {
    tx_hash: String,
    block_number: u64,
    timestamp: u64,
    slot: u32,
    kind: String,
    asset: AssetResponse,
    from: String,
    to: String,
    amount: String,
    tainted: String,
    share_bps: u64,
    source: Option<TagSourceResponse>,
}

impl From<&TaintedMove> for TaintFlowResponse {
    fn from(moved: &TaintedMove) -> Self {
        Self {
            tx_hash: moved.tx_hash().to_string(),
            block_number: moved.meta().block_number(),
            timestamp: moved.meta().timestamp(),
            slot: moved.slot(),
            kind: moved.kind().as_str().to_owned(),
            asset: moved.asset().into(),
            from: moved.from().to_string(),
            to: moved.to().to_string(),
            amount: moved.amount().to_string(),
            tainted: moved.tainted().to_string(),
            share_bps: moved.share_bps(),
            source: moved.source().as_ref().map(TagSourceResponse::from),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct TaintHoldingResponse {
    address: String,
    asset: AssetResponse,
    total: String,
    tainted: String,
}

impl TaintHoldingResponse {
    fn new(address: &EthAddress, asset: &Asset, holding: &Holding) -> Self {
        Self {
            address: address.to_string(),
            asset: asset.into(),
            total: holding.total().to_string(),
            tainted: holding.tainted().to_string(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct TaintResponse {
    policy: String,
    min_share_bps: u64,
    seeds: Vec<String>,
    sinks: Vec<String>,
    transit: Vec<String>,
    flows: Vec<TaintFlowResponse>,
    holdings: Vec<TaintHoldingResponse>,
}

#[derive(Serialize, ToSchema)]
pub struct TaintGraphResponse {
    #[serde(flatten)]
    graph: GraphResponse,
    taint: TaintResponse,
}

impl TaintGraphResponse {
    pub fn new(
        traced: &TaintGraph,
        span: BlockRange,
        extra: &ExtraLabels,
        min_share_bps: u64,
    ) -> Self {
        let trace = traced.trace();
        let mut transit: Vec<String> = trace
            .transit()
            .iter()
            .filter(|pool| {
                trace
                    .moves()
                    .iter()
                    .any(|moved| moved.from() == *pool || moved.to() == *pool)
            })
            .map(ToString::to_string)
            .collect();
        transit.sort();

        Self {
            graph: GraphResponse::new(traced.graph(), span, extra),
            taint: TaintResponse {
                policy: "haircut".to_owned(),
                min_share_bps,
                seeds: traced.seeds().iter().map(ToString::to_string).collect(),
                sinks: traced.sinks().iter().map(ToString::to_string).collect(),
                transit,
                flows: trace.moves().iter().map(TaintFlowResponse::from).collect(),
                holdings: trace
                    .held()
                    .iter()
                    .map(|(address, asset, holding)| {
                        TaintHoldingResponse::new(address, asset, holding)
                    })
                    .collect(),
            },
        }
    }
}
