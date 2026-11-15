pub mod account;
pub mod info;
pub mod monitor;
pub mod tls;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use application::eth::{
    AddressGraph, ExploreLimits, GraphNode, LowLevelGraph, LowLevelNode, RpcPlan,
};
use domain::eth::{
    Actor, ActorKind, AddressLabel, BlockBucket, BlockRange, ContractAction, ContractKind,
    EthAddress, IndexCoverage, Interaction, InteractionEdge, InteractionKind, LowLevelInteraction,
    Tag, TagSubject,
};

use crate::state::{AppState, ChainState};

const DEFAULT_DEPTH: u32 = 1;

const DEFAULT_DECIMALS: u8 = 18;

type Tokens<'a> = HashMap<&'a EthAddress, (&'a str, u8)>;

pub type ExtraLabels = HashMap<EthAddress, Vec<AddressLabel>>;

fn labels_of(
    address: &EthAddress,
    own: &[AddressLabel],
    extra: &ExtraLabels,
) -> Vec<AddressLabelResponse> {
    extra
        .get(address)
        .into_iter()
        .flatten()
        .chain(own)
        .map(AddressLabelResponse::from)
        .collect()
}

fn tokens_of(actors: &[Actor]) -> Tokens<'_> {
    actors
        .iter()
        .filter_map(|actor| Some((actor.address(), actor.erc20()?)))
        .collect()
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum ChainParam {
    Id(u64),
    Name(String),
}

impl ChainParam {
    pub fn selector(&self) -> String {
        match self {
            Self::Id(chain_id) => chain_id.to_string(),
            Self::Name(name) => name.clone(),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GraphRequest {
    #[serde(default)]
    pub chain: Option<ChainParam>,
    #[serde(default)]
    pub scope: ScopeRequest,
    #[serde(default)]
    pub roots: Vec<RootRequest>,
    pub from_block: u64,
    pub to_block: u64,

    #[serde(default)]
    pub ranges: Vec<RangeRequest>,

    #[serde(default)]
    pub confirm_rpc: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScopeRequest {
    #[default]
    Roots,
    Blocks,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RangeRequest {
    pub from_block: u64,
    pub to_block: u64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RootRequest {
    pub address: String,

    #[serde(default = "default_depth")]
    pub depth: u32,
}

fn default_depth() -> u32 {
    DEFAULT_DEPTH
}

#[derive(Debug, Serialize, ToSchema)]
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

#[derive(Serialize, ToSchema)]
pub struct GraphResponse {
    nodes: Vec<NodeResponse>,
    edges: Vec<EdgeResponse>,
    tags: Vec<TagResponse>,
    span: RangeResponse,
    filled_from_rpc: Vec<RangeResponse>,
    truncated: bool,
}

impl GraphResponse {
    pub fn new(graph: &AddressGraph, span: BlockRange, extra: &ExtraLabels) -> Self {
        let tokens = tokens_of(graph.actors());

        Self {
            nodes: graph
                .nodes()
                .iter()
                .map(|node| NodeResponse::new(node, extra))
                .collect(),
            edges: graph
                .edges()
                .iter()
                .map(|edge| EdgeResponse::new(edge, &tokens))
                .collect(),
            tags: graph.tags().iter().map(TagResponse::from).collect(),
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

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TagSubjectKind {
    Address,
    Tx,
}

#[derive(Serialize, ToSchema)]
pub struct TagSourceResponse {
    kind: String,
    name: String,
    version: u32,
}

#[derive(Serialize, ToSchema)]
pub struct TagResponse {
    subject_kind: TagSubjectKind,
    subject: String,
    key: String,
    category: String,
    confidence: f32,
    source: TagSourceResponse,
    evidence: Vec<String>,
    note: String,
}

impl From<&Tag> for TagResponse {
    fn from(tag: &Tag) -> Self {
        let (subject_kind, subject) = match tag.subject() {
            TagSubject::Address(address) => (TagSubjectKind::Address, address.to_string()),
            TagSubject::Tx(hash) => (TagSubjectKind::Tx, hash.to_string()),
        };
        Self {
            subject_kind,
            subject,
            key: tag.key().to_owned(),
            category: tag.category().to_owned(),
            confidence: tag.confidence(),
            source: TagSourceResponse {
                kind: tag.source().kind().as_str().to_owned(),
                name: tag.source().name().to_owned(),
                version: tag.source().version(),
            },
            evidence: tag.evidence().iter().map(ToString::to_string).collect(),
            note: tag.note().to_owned(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct LowLevelGraphResponse {
    actors: Vec<LowLevelActorResponse>,
    interactions: Vec<LowLevelInteractionResponse>,
    span: RangeResponse,
    filled_from_rpc: Vec<RangeResponse>,
    truncated: bool,
}

impl LowLevelGraphResponse {
    pub fn new(graph: &LowLevelGraph, span: BlockRange, extra: &ExtraLabels) -> Self {
        Self {
            actors: graph
                .actors()
                .iter()
                .map(|node| LowLevelActorResponse::new(node, extra))
                .collect(),
            interactions: graph
                .interactions()
                .iter()
                .map(LowLevelInteractionResponse::from)
                .collect(),
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

#[derive(Serialize, ToSchema)]
pub struct AddressLabelResponse {
    value: String,
    source: String,
}

impl From<&AddressLabel> for AddressLabelResponse {
    fn from(label: &AddressLabel) -> Self {
        Self {
            value: label.value().to_owned(),
            source: label.source().to_owned(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct LowLevelActorResponse {
    address: String,
    labels: Vec<AddressLabelResponse>,
    depth: u32,
    root: bool,
    expanded: bool,
}

impl LowLevelActorResponse {
    pub fn new(node: &LowLevelNode, extra: &ExtraLabels) -> Self {
        Self {
            address: node.address().to_string(),
            labels: labels_of(node.address(), node.actor().labels(), extra),
            depth: node.depth(),
            root: node.root(),
            expanded: node.expanded(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct LowLevelInteractionResponse {
    tx_hash: String,
    block_number: u64,
    timestamp: u64,
    succeeded: bool,
    from: String,
    to: String,
    amount: String,
    deployment: bool,
}

impl From<&LowLevelInteraction> for LowLevelInteractionResponse {
    fn from(interaction: &LowLevelInteraction) -> Self {
        let meta = interaction.meta();

        Self {
            tx_hash: meta.tx_hash().to_string(),
            block_number: meta.block_number(),
            timestamp: meta.timestamp(),
            succeeded: interaction.succeeded(),
            from: interaction.from().to_string(),
            to: interaction.to().to_string(),
            amount: interaction.amount().to_string(),
            deployment: interaction.is_deployment(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct NodeResponse {
    address: String,
    labels: Vec<AddressLabelResponse>,
    depth: u32,
    root: bool,
    expanded: bool,

    #[serde(flatten)]
    actor: ActorResponse,
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActorResponse {
    Eoa,
    Contract,
    Erc20 { symbol: String, decimals: u8 },
    Unknown,
}

impl NodeResponse {
    pub fn new(node: &GraphNode, extra: &ExtraLabels) -> Self {
        Self {
            address: node.address().to_string(),
            labels: labels_of(node.address(), node.actor().labels(), extra),
            depth: node.depth(),
            root: node.root(),
            expanded: node.expanded(),
            actor: node.actor().kind().into(),
        }
    }
}

impl From<&ActorKind> for ActorResponse {
    fn from(kind: &ActorKind) -> Self {
        match kind {
            ActorKind::Eoa => Self::Eoa,
            ActorKind::Contract(ContractKind::Plain) => Self::Contract,
            ActorKind::Contract(ContractKind::Erc20 { symbol, decimals }) => Self::Erc20 {
                symbol: symbol.clone(),
                decimals: *decimals,
            },
            ActorKind::Unknown => Self::Unknown,
        }
    }
}

#[derive(Serialize, ToSchema)]
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

#[derive(Serialize, ToSchema)]
pub struct ChainsResponse {
    default_chain: u64,
    chains: Vec<ChainResponse>,
}

impl ChainsResponse {
    pub fn new(state: &AppState) -> Self {
        Self {
            default_chain: state.default_chain(),
            chains: state.chains().map(ChainResponse::new).collect(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ChainResponse {
    chain_id: u64,
    family: String,
    network: String,
    name: String,
    slug: String,
    native_symbol: String,
    native_decimals: u8,
    l2_of: Option<u64>,
    rpc_nodes: usize,
}

impl ChainResponse {
    fn new(chain: &ChainState) -> Self {
        let meta = chain.meta();

        Self {
            chain_id: meta.chain_id,
            family: meta.family.name().to_owned(),
            network: meta.network.name().to_owned(),
            name: meta.name.clone(),
            slug: meta.slug.clone(),
            native_symbol: meta.native_symbol.clone(),
            native_decimals: meta.native_decimals,
            l2_of: meta.l2_of,
            rpc_nodes: chain.rpc_nodes(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct CoverageResponse {
    chain_id: u64,
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
        chain_id: u64,
        persisted: bool,
        chain_head: Option<u64>,
        limits: ExploreLimits,
    ) -> Self {
        Self {
            chain_id,
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

#[derive(Serialize, ToSchema)]
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

#[derive(Serialize, ToSchema)]
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

#[derive(Serialize, ToSchema)]
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

#[derive(Serialize, ToSchema)]
pub struct EdgeResponse {
    tx_hash: String,
    block_number: u64,
    timestamp: u64,
    succeeded: bool,

    #[serde(flatten)]
    interaction: InteractionResponse,
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionResponse {
    Protocol,
    NativeTransfer {
        from: String,
        to: String,
        amount: String,
    },
    AssetTransfer {
        asset_id: String,
        symbol: String,
        decimals: u8,
        from: String,
        to: String,
        amount: String,
    },
    SystemAction {
        action: String,
        from: String,
        to: Option<String>,
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

#[derive(Serialize, ToSchema)]
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

impl EdgeResponse {
    pub fn new(edge: &InteractionEdge, tokens: &Tokens<'_>) -> Self {
        let meta = edge.meta();
        Self {
            tx_hash: meta.tx_hash().to_string(),
            block_number: meta.block_number(),
            timestamp: meta.timestamp(),
            succeeded: edge.interaction().receipt().succeeded(),
            interaction: InteractionResponse::new(edge.interaction(), tokens),
        }
    }
}

impl InteractionResponse {
    fn new(interaction: &Interaction, tokens: &Tokens<'_>) -> Self {
        match interaction.kind() {
            InteractionKind::Protocol => Self::Protocol,
            InteractionKind::NativeTransfer(transfer) => Self::NativeTransfer {
                from: transfer.from().to_string(),
                to: transfer.to().to_string(),
                amount: transfer.amount().to_string(),
            },
            InteractionKind::AssetTransfer(transfer) => Self::AssetTransfer {
                asset_id: transfer.asset().id().to_owned(),
                symbol: transfer.asset().symbol().to_owned(),
                decimals: transfer.asset().decimals(),
                from: transfer.from().to_string(),
                to: transfer.to().to_string(),
                amount: transfer.amount().to_string(),
            },
            InteractionKind::SystemAction {
                action,
                from,
                to,
                amount,
            } => Self::SystemAction {
                action: action.clone(),
                from: from.to_string(),
                to: to.map(|to| to.to_string()),
                amount: amount.to_string(),
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
                action: ContractActionResponse::new(contract_interaction_type, tokens),
            },
        }
    }
}

impl ContractActionResponse {
    fn new(action: &ContractAction, tokens: &Tokens<'_>) -> Self {
        match action {
            ContractAction::Erc20Transfer {
                token,
                from,
                to,
                amount,
            } => {
                let (token_name, decimals) = tokens
                    .get(token)
                    .map(|(symbol, decimals)| ((*symbol).to_owned(), *decimals))
                    .unwrap_or_else(|| (token.to_string(), DEFAULT_DECIMALS));

                Self::Erc20Transfer {
                    token: token.to_string(),
                    from: from.to_string(),
                    to: to.to_string(),
                    amount: amount.to_string(),
                    token_name,
                    decimals,
                }
            }
            ContractAction::Other => Self::Other,
        }
    }
}

pub fn parse_address(value: &str) -> Result<EthAddress, String> {
    EthAddress::parse_any(value).map_err(|error| format!("{value} is not an address: {error}"))
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
                },
            },
        );

        vec![
            InteractionEdge::new(meta.clone(), 0, native),
            InteractionEdge::new(meta, 1, erc20),
        ]
    }

    fn usdc() -> Vec<Actor> {
        vec![Actor::new(
            "0x3333333333333333333333333333333333333333"
                .parse()
                .unwrap(),
            ActorKind::Contract(ContractKind::Erc20 {
                symbol: "USDC".to_owned(),
                decimals: 6,
            }),
            Vec::new(),
        )]
    }

    #[test]
    fn an_edge_keeps_its_wire_shape() {
        let cast = usdc();
        let tokens = tokens_of(&cast);
        let encoded: Vec<serde_json::Value> = edges()
            .iter()
            .map(|edge| EdgeResponse::new(edge, &tokens))
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

    fn raw_edge(to: Option<EthAddress>, created: Option<EthAddress>) -> LowLevelInteraction {
        let mined = domain::eth::MinedTx::new(
            {
                let builder = EthTx::builder()
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
                    .data(Bytes::new());

                match to {
                    Some(to) => builder.to(to).build(),
                    None => builder.build(),
                }
            },
            EthReceipt::new(true, created, Vec::new()),
        );

        LowLevelInteraction::try_from(&mined).unwrap()
    }

    #[test]
    fn a_low_level_edge_keeps_its_wire_shape() {
        let edge = raw_edge(
            Some(
                "0x2222222222222222222222222222222222222222"
                    .parse()
                    .unwrap(),
            ),
            None,
        );

        assert_eq!(
            serde_json::to_value(LowLevelInteractionResponse::from(&edge)).unwrap(),
            json!({
                "tx_hash": "0xabababababababababababababababababababababababababababababababab",
                "block_number": 21_000_000,
                "timestamp": 1_737_000_000,
                "succeeded": true,
                "from": "0x1111111111111111111111111111111111111111",
                "to": "0x2222222222222222222222222222222222222222",
                "amount": "1500000000000000000",
                "deployment": false
            })
        );
    }

    #[test]
    fn a_low_level_deployment_points_at_the_contract_and_says_so() {
        let edge = raw_edge(
            None,
            Some(
                "0x4444444444444444444444444444444444444444"
                    .parse()
                    .unwrap(),
            ),
        );
        let encoded = serde_json::to_value(LowLevelInteractionResponse::from(&edge)).unwrap();

        assert_eq!(encoded["deployment"], json!(true));
        assert_eq!(
            encoded["to"],
            json!("0x4444444444444444444444444444444444444444")
        );
    }

    #[test]
    fn a_low_level_actor_carries_the_labels_it_was_given() {
        let node = LowLevelNode::new(
            domain::eth::LowLevelActor::new(
                "0x1111111111111111111111111111111111111111"
                    .parse()
                    .unwrap(),
                vec![AddressLabel::new(
                    "Binance 7".to_owned(),
                    "etherscan.io".to_owned(),
                )],
            ),
            2,
            false,
            true,
        );

        assert_eq!(
            serde_json::to_value(LowLevelActorResponse::new(&node, &ExtraLabels::new())).unwrap(),
            json!({
                "address": "0x1111111111111111111111111111111111111111",
                "labels": [{"value": "Binance 7", "source": "etherscan.io"}],
                "depth": 2,
                "root": false,
                "expanded": true
            })
        );
    }

    #[test]
    fn your_labels_come_before_the_ones_the_server_knows() {
        let address: EthAddress = "0x1111111111111111111111111111111111111111"
            .parse()
            .unwrap();
        let node = LowLevelNode::new(
            domain::eth::LowLevelActor::new(
                address,
                vec![AddressLabel::new(
                    "Binance 7".to_owned(),
                    "etherscan.io".to_owned(),
                )],
            ),
            0,
            true,
            true,
        );
        let extra = ExtraLabels::from([(
            address,
            vec![AddressLabel::new("Suspect".to_owned(), "you".to_owned())],
        )]);

        assert_eq!(
            serde_json::to_value(LowLevelActorResponse::new(&node, &extra)).unwrap()["labels"],
            json!([
                {"value": "Suspect", "source": "you"},
                {"value": "Binance 7", "source": "etherscan.io"}
            ])
        );
    }

    #[test]
    fn a_low_level_actor_nobody_labelled_carries_an_empty_list() {
        let node = LowLevelNode::new(
            domain::eth::LowLevelActor::new(
                "0x1111111111111111111111111111111111111111"
                    .parse()
                    .unwrap(),
                Vec::new(),
            ),
            2,
            false,
            true,
        );

        assert_eq!(
            serde_json::to_value(LowLevelActorResponse::new(&node, &ExtraLabels::new())).unwrap()["labels"],
            json!([])
        );
    }

    #[test]
    fn a_token_nobody_named_falls_back_to_its_address() {
        let tokens = tokens_of(&[]);
        let encoded = serde_json::to_value(EdgeResponse::new(&edges()[1], &tokens)).unwrap();

        assert_eq!(
            encoded["action"]["token_name"],
            json!("0x3333333333333333333333333333333333333333")
        );
        assert_eq!(encoded["action"]["decimals"], json!(18));
    }

    #[test]
    fn a_node_wears_the_kind_of_its_actor() {
        let encoded = serde_json::to_value(ActorResponse::from(&ActorKind::Contract(
            ContractKind::Erc20 {
                symbol: "USDC".to_owned(),
                decimals: 6,
            },
        )))
        .unwrap();

        assert_eq!(
            encoded,
            json!({ "kind": "erc20", "symbol": "USDC", "decimals": 6 })
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
            1,
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
