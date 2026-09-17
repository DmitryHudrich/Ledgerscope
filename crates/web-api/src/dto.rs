use serde::{Deserialize, Serialize};

use application::eth::{AddressGraph, ExploreLimits, GraphNode, RpcPlan};
use domain::eth::{
    BlockBucket, BlockRange, ContractAction, EthAddress, IndexCoverage, Interaction,
    InteractionEdge, InteractionKind,
};

const DEFAULT_DEPTH: u32 = 1;

#[derive(Debug, Deserialize)]
pub struct GraphRequest {
    pub roots: Vec<RootRequest>,
    pub from_block: u64,
    pub to_block: u64,

    #[serde(default)]
    pub confirm_rpc: bool,
}

#[derive(Debug, Deserialize)]
pub struct RootRequest {
    pub address: String,

    #[serde(default = "default_depth")]
    pub depth: u32,
}

fn default_depth() -> u32 {
    DEFAULT_DEPTH
}

#[derive(Debug, Serialize)]
pub struct RangeResponse {
    from_block: u64,
    to_block: u64,
    block_count: u64,
}

impl From<BlockRange> for RangeResponse {
    fn from(range: BlockRange) -> Self {
        Self {
            from_block: range.from_block(),
            to_block: range.to_block(),
            block_count: range.block_count(),
        }
    }
}

#[derive(Serialize)]
pub struct GraphResponse {
    nodes: Vec<NodeResponse>,
    edges: Vec<EdgeResponse>,
    span: RangeResponse,
    filled_from_rpc: Vec<RangeResponse>,
    truncated: bool,
}

impl GraphResponse {
    pub fn new(graph: &AddressGraph, span: BlockRange) -> Self {
        Self {
            nodes: graph.nodes().iter().map(NodeResponse::from).collect(),
            edges: graph.edges().iter().map(EdgeResponse::from).collect(),
            span: span.into(),
            filled_from_rpc: graph
                .filled()
                .iter()
                .copied()
                .map(RangeResponse::from)
                .collect(),
            truncated: graph.truncated(),
        }
    }
}

#[derive(Serialize)]
pub struct NodeResponse {
    address: String,
    depth: u32,
    root: bool,
    expanded: bool,
}

impl From<&GraphNode> for NodeResponse {
    fn from(node: &GraphNode) -> Self {
        Self {
            address: node.address().to_string(),
            depth: node.depth(),
            root: node.root(),
            expanded: node.expanded(),
        }
    }
}

#[derive(Serialize)]
pub struct RpcConfirmationResponse {
    status: &'static str,
    message: String,
    span: RangeResponse,
    missing: Vec<RangeResponse>,
    missing_blocks: u64,
    indexed_blocks: u64,
}

impl RpcConfirmationResponse {
    pub fn new(plan: &RpcPlan, span: BlockRange) -> Self {
        Self {
            status: "rpc_confirmation_required",
            message: format!(
                "{} of {} blocks are not in clickhouse yet, \
                 send the same request with confirm_rpc to ask the node for them",
                plan.missing_blocks(),
                span.block_count()
            ),
            span: span.into(),
            missing: plan
                .missing()
                .iter()
                .copied()
                .map(RangeResponse::from)
                .collect(),
            missing_blocks: plan.missing_blocks(),
            indexed_blocks: plan.indexed_blocks(),
        }
    }
}

#[derive(Serialize)]
pub struct CoverageResponse {
    persisted: bool,
    chain_head: Option<u64>,
    lowest_block: Option<u64>,
    highest_block: Option<u64>,
    block_count: u64,
    tx_count: u64,
    ranges: Vec<RangeResponse>,
    limits: LimitsResponse,
}

impl CoverageResponse {
    pub fn new(
        coverage: &IndexCoverage,
        persisted: bool,
        chain_head: Option<u64>,
        limits: ExploreLimits,
    ) -> Self {
        Self {
            persisted,
            chain_head,
            lowest_block: coverage.lowest_block(),
            highest_block: coverage.highest_block(),
            block_count: coverage.block_count(),
            tx_count: coverage.tx_count(),
            ranges: coverage
                .ranges()
                .iter()
                .copied()
                .map(RangeResponse::from)
                .collect(),
            limits: limits.into(),
        }
    }
}

#[derive(Serialize)]
pub struct LimitsResponse {
    max_roots: usize,
    max_depth: u32,
    max_nodes: usize,
    max_edges: usize,
    max_blocks: u64,
}

impl From<ExploreLimits> for LimitsResponse {
    fn from(limits: ExploreLimits) -> Self {
        Self {
            max_roots: limits.max_roots,
            max_depth: limits.max_depth,
            max_nodes: limits.max_nodes,
            max_edges: limits.max_edges,
            max_blocks: limits.max_blocks,
        }
    }
}

#[derive(Serialize)]
pub struct HistogramResponse {
    span: Option<RangeResponse>,
    bucket_size: u64,
    buckets: Vec<BucketResponse>,
}

impl HistogramResponse {
    pub fn new(span: Option<BlockRange>, buckets: Vec<BlockBucket>) -> Self {
        Self {
            span: span.map(RangeResponse::from),
            bucket_size: buckets
                .first()
                .map(|bucket| bucket.range().block_count())
                .unwrap_or_default(),
            buckets: buckets.iter().map(BucketResponse::from).collect(),
        }
    }
}

#[derive(Serialize)]
pub struct BucketResponse {
    from_block: u64,
    to_block: u64,
    indexed_blocks: u64,
    tx_count: u64,
}

impl From<&BlockBucket> for BucketResponse {
    fn from(bucket: &BlockBucket) -> Self {
        Self {
            from_block: bucket.range().from_block(),
            to_block: bucket.range().to_block(),
            indexed_blocks: bucket.indexed_blocks(),
            tx_count: bucket.tx_count(),
        }
    }
}

#[derive(Serialize)]
pub struct EdgeResponse {
    tx_hash: String,
    block_number: u64,
    timestamp: u64,
    succeeded: bool,

    #[serde(flatten)]
    interaction: InteractionResponse,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionResponse {
    Protocol,
    NativeTransfer {
        from: String,
        to: String,
        amount: String,
    },
    ContractDeployment {
        deployer: String,
        contract_address: String,
    },
    ContractInteraction {
        interactor: String,
        contract_address: String,
        action: ContractActionResponse,
    },
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContractActionResponse {
    Erc20Transfer {
        token: String,
        from: String,
        to: String,
        amount: String,
        token_name: String,
        decimals: u8,
    },
    Other,
}

impl From<&InteractionEdge> for EdgeResponse {
    fn from(edge: &InteractionEdge) -> Self {
        let meta = edge.meta();
        Self {
            tx_hash: meta.tx_hash().to_string(),
            block_number: meta.block_number(),
            timestamp: meta.timestamp(),
            succeeded: edge.interaction().receipt().succeeded(),
            interaction: edge.interaction().into(),
        }
    }
}

impl From<&Interaction> for InteractionResponse {
    fn from(interaction: &Interaction) -> Self {
        interaction.kind().into()
    }
}

impl From<&InteractionKind> for InteractionResponse {
    fn from(kind: &InteractionKind) -> Self {
        match kind {
            InteractionKind::Protocol => Self::Protocol,
            InteractionKind::NativeTransfer(transfer) => Self::NativeTransfer {
                from: transfer.from().to_string(),
                to: transfer.to().to_string(),
                amount: transfer.amount().to_string(),
            },
            InteractionKind::ContractDeployment {
                contract_address,
                deployer,
            } => Self::ContractDeployment {
                deployer: deployer.to_string(),
                contract_address: contract_address.to_string(),
            },
            InteractionKind::ContractInteraction {
                contract_address,
                interactor,
                contract_interaction_type,
            } => Self::ContractInteraction {
                interactor: interactor.to_string(),
                contract_address: contract_address.to_string(),
                action: contract_interaction_type.into(),
            },
        }
    }
}

impl From<&ContractAction> for ContractActionResponse {
    fn from(action: &ContractAction) -> Self {
        match action {
            ContractAction::Erc20Transfer {
                token,
                from,
                to,
                amount,
                token_name,
                decimals,
            } => Self::Erc20Transfer {
                token: token.to_string(),
                from: from.to_string(),
                to: to.to_string(),
                amount: amount.to_string(),
                token_name: token_name.clone(),
                decimals: *decimals,
            },
            ContractAction::Other => Self::Other,
        }
    }
}

pub fn parse_address(value: &str) -> Result<EthAddress, String> {
    value
        .trim()
        .parse::<EthAddress>()
        .map_err(|error| format!("{value} is not an eth address: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{Bytes, U256, b256};
    use domain::eth::{EthReceipt, EthTx, TxMeta, graph::NativeTransfer};
    use serde_json::json;

    fn edges() -> Vec<InteractionEdge> {
        let tx = EthTx::builder()
            .tx_hash(b256!(
                "0xabababababababababababababababababababababababababababababababab"
            ))
            .block_number(21_000_000)
            .timestamp(1_737_000_000)
            .amount(U256::from(1_500_000_000_000_000_000u128))
            .from(
                "0x1111111111111111111111111111111111111111"
                    .parse()
                    .unwrap(),
            )
            .to("0x2222222222222222222222222222222222222222"
                .parse()
                .unwrap())
            .data(Bytes::new())
            .build();
        let meta = TxMeta::from(&tx);
        let receipt = EthReceipt::new(true, None, Vec::new());

        let native = Interaction::new(
            receipt.clone(),
            InteractionKind::NativeTransfer(NativeTransfer::try_from(tx).ok().unwrap()),
        );

        let erc20 = Interaction::new(
            receipt,
            InteractionKind::ContractInteraction {
                contract_address: "0x3333333333333333333333333333333333333333"
                    .parse()
                    .unwrap(),
                interactor: "0x1111111111111111111111111111111111111111"
                    .parse()
                    .unwrap(),
                contract_interaction_type: ContractAction::Erc20Transfer {
                    token: "0x3333333333333333333333333333333333333333"
                        .parse()
                        .unwrap(),
                    from: "0x1111111111111111111111111111111111111111"
                        .parse()
                        .unwrap(),
                    to: "0x2222222222222222222222222222222222222222"
                        .parse()
                        .unwrap(),
                    amount: U256::from(1_000_000),
                    token_name: "USDC".to_owned(),
                    decimals: 6,
                },
            },
        );

        vec![
            InteractionEdge::new(meta.clone(), 0, native),
            InteractionEdge::new(meta, 1, erc20),
        ]
    }

    #[test]
    fn an_edge_keeps_its_wire_shape() {
        let encoded: Vec<serde_json::Value> = edges()
            .iter()
            .map(EdgeResponse::from)
            .map(|edge| serde_json::to_value(&edge).unwrap())
            .collect();

        assert_eq!(
            encoded,
            vec![
                json!({
                    "tx_hash": "0xabababababababababababababababababababababababababababababababab",
                    "block_number": 21_000_000,
                    "timestamp": 1_737_000_000,
                    "succeeded": true,
                    "kind": "native_transfer",
                    "from": "0x1111111111111111111111111111111111111111",
                    "to": "0x2222222222222222222222222222222222222222",
                    "amount": "1500000000000000000"
                }),
                json!({
                    "tx_hash": "0xabababababababababababababababababababababababababababababababab",
                    "block_number": 21_000_000,
                    "timestamp": 1_737_000_000,
                    "succeeded": true,
                    "kind": "contract_interaction",
                    "interactor": "0x1111111111111111111111111111111111111111",
                    "contract_address": "0x3333333333333333333333333333333333333333",
                    "action": {
                        "kind": "erc20_transfer",
                        "token": "0x3333333333333333333333333333333333333333",
                        "from": "0x1111111111111111111111111111111111111111",
                        "to": "0x2222222222222222222222222222222222222222",
                        "amount": "1000000",
                        "token_name": "USDC",
                        "decimals": 6
                    }
                })
            ]
        );
    }

    #[test]
    fn a_confirmation_names_every_hole() {
        let span = BlockRange::new(10, 30);
        let plan = RpcPlan::new(vec![BlockRange::new(21, 30)], 11, 10);

        assert_eq!(
            serde_json::to_value(RpcConfirmationResponse::new(&plan, span)).unwrap()["missing"],
            json!([{ "from_block": 21, "to_block": 30, "block_count": 10 }])
        );
    }

    #[test]
    fn coverage_carries_the_limits_the_ui_needs() {
        let coverage = IndexCoverage::new(vec![BlockRange::new(10, 20)], 42);
        let encoded = serde_json::to_value(CoverageResponse::new(
            &coverage,
            true,
            Some(21_000_000),
            ExploreLimits::default(),
        ))
        .unwrap();

        assert_eq!(encoded["lowest_block"], json!(10));
        assert_eq!(encoded["highest_block"], json!(20));
        assert_eq!(encoded["block_count"], json!(11));
        assert_eq!(encoded["tx_count"], json!(42));
        assert_eq!(encoded["chain_head"], json!(21_000_000));
        assert_eq!(
            encoded["limits"]["max_depth"],
            json!(ExploreLimits::default().max_depth)
        );
    }

    #[test]
    fn a_histogram_reports_its_bucket_size() {
        let buckets = vec![
            BlockBucket::new(BlockRange::new(0, 9), 10, 100),
            BlockBucket::new(BlockRange::new(10, 19), 4, 7),
        ];
        let encoded = serde_json::to_value(HistogramResponse::new(
            Some(BlockRange::new(0, 19)),
            buckets,
        ))
        .unwrap();

        assert_eq!(encoded["bucket_size"], json!(10));
        assert_eq!(encoded["buckets"][1]["tx_count"], json!(7));
    }
}
