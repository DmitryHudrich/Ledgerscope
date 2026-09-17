use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    fmt, io,
    sync::Arc,
};

use futures::StreamExt;

use domain::eth::{
    BlockBucket, BlockRange, EthAddress, IndexCoverage, Interaction, InteractionEdge,
    InteractionId, MinedTx, TxMeta,
};

use crate::eth::{
    classificator::EthTxClassificator,
    ports::{EthTxIndex, EthTxSource},
};

const ASKING_CHUNK: usize = 128;
const CLASSIFY_CONCURRENCY: usize = 20;

#[derive(Clone, Copy, Debug)]
pub struct GraphRoot {
    address: EthAddress,
    depth: u32,
}

impl GraphRoot {
    pub fn new(address: EthAddress, depth: u32) -> Self {
        Self { address, depth }
    }

    pub fn address(&self) -> &EthAddress {
        &self.address
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }
}

#[derive(Clone, Debug)]
pub struct ExploreRequest {
    roots: Vec<GraphRoot>,
    span: BlockRange,
    confirm_rpc: bool,
}

impl ExploreRequest {
    pub fn new(roots: Vec<GraphRoot>, span: BlockRange, confirm_rpc: bool) -> Self {
        Self {
            roots,
            span,
            confirm_rpc,
        }
    }

    pub fn roots(&self) -> &[GraphRoot] {
        &self.roots
    }

    pub fn span(&self) -> BlockRange {
        self.span
    }

    pub fn confirm_rpc(&self) -> bool {
        self.confirm_rpc
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ExploreLimits {
    pub max_roots: usize,
    pub max_depth: u32,
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_blocks: u64,
}

impl Default for ExploreLimits {
    fn default() -> Self {
        Self {
            max_roots: 64,
            max_depth: 5,
            max_nodes: 5_000,
            max_edges: 20_000,
            max_blocks: 100_000,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GraphNode {
    address: EthAddress,
    depth: u32,
    root: bool,
    expanded: bool,
}

impl GraphNode {
    pub fn address(&self) -> &EthAddress {
        &self.address
    }

    pub fn depth(&self) -> u32 {
        self.depth
    }

    pub fn root(&self) -> bool {
        self.root
    }

    pub fn expanded(&self) -> bool {
        self.expanded
    }
}

#[derive(Debug, Default)]
pub struct AddressGraph {
    nodes: Vec<GraphNode>,
    edges: Vec<InteractionEdge>,
    filled: Vec<BlockRange>,
    truncated: bool,
}

impl AddressGraph {
    pub fn nodes(&self) -> &[GraphNode] {
        &self.nodes
    }

    pub fn edges(&self) -> &[InteractionEdge] {
        &self.edges
    }

    pub fn filled(&self) -> &[BlockRange] {
        &self.filled
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

#[derive(Clone, Debug)]
pub struct RpcPlan {
    missing: Vec<BlockRange>,
    indexed_blocks: u64,
    missing_blocks: u64,
}

impl RpcPlan {
    pub fn new(missing: Vec<BlockRange>, indexed_blocks: u64, missing_blocks: u64) -> Self {
        Self {
            missing,
            indexed_blocks,
            missing_blocks,
        }
    }

    pub fn missing(&self) -> &[BlockRange] {
        &self.missing
    }

    pub fn indexed_blocks(&self) -> u64 {
        self.indexed_blocks
    }

    pub fn missing_blocks(&self) -> u64 {
        self.missing_blocks
    }
}

#[derive(Debug)]
pub enum Exploration {
    Graph(Box<AddressGraph>),
    RpcNeeded(RpcPlan),
}

#[derive(Debug)]
pub enum ExploreError {
    NoRoots,
    TooManyRoots(usize),
    TooDeep(u32),
    SpanTooWide(u64),
    Io(io::Error),
}

impl fmt::Display for ExploreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExploreError::NoRoots => write!(f, "ask for at least one address"),
            ExploreError::TooManyRoots(limit) => write!(f, "at most {limit} addresses at a time"),
            ExploreError::TooDeep(limit) => write!(f, "depth stops at {limit}"),
            ExploreError::SpanTooWide(limit) => write!(f, "at most {limit} blocks in one span"),
            ExploreError::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ExploreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ExploreError::Io(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct Walked {
    depth: u32,
    budget: u32,
}

pub struct EthExplorer {
    index: Arc<dyn EthTxIndex>,
    source: Arc<dyn EthTxSource>,
    classificator: Arc<dyn EthTxClassificator>,
    limits: ExploreLimits,
}

impl EthExplorer {
    pub fn new(
        index: Arc<dyn EthTxIndex>,
        source: Arc<dyn EthTxSource>,
        classificator: Arc<dyn EthTxClassificator>,
        limits: ExploreLimits,
    ) -> Self {
        Self {
            index,
            source,
            classificator,
            limits,
        }
    }

    pub fn limits(&self) -> ExploreLimits {
        self.limits
    }

    pub fn persistent(&self) -> bool {
        self.index.persistent()
    }

    pub async fn coverage(&self, span: BlockRange) -> Result<IndexCoverage, io::Error> {
        self.index.coverage(span).await
    }

    pub async fn histogram(
        &self,
        span: BlockRange,
        buckets: u32,
    ) -> Result<Vec<BlockBucket>, io::Error> {
        self.index.histogram(span, buckets).await
    }

    pub async fn explore(&self, request: ExploreRequest) -> Result<Exploration, ExploreError> {
        self.weigh(&request)?;

        let coverage = self
            .index
            .coverage(request.span)
            .await
            .map_err(ExploreError::Io)?;
        let missing = coverage.gaps(request.span);

        if !missing.is_empty() && !request.confirm_rpc {
            return Ok(Exploration::RpcNeeded(RpcPlan::new(
                missing.clone(),
                coverage.indexed_within(request.span),
                missing.iter().map(BlockRange::block_count).sum(),
            )));
        }

        if !missing.is_empty() && self.index.persistent() {
            for gap in &missing {
                self.fill(*gap).await.map_err(ExploreError::Io)?;
            }
        }

        let mut graph = self.walk(&request).await.map_err(ExploreError::Io)?;
        graph.filled = missing;

        Ok(Exploration::Graph(Box::new(graph)))
    }

    fn weigh(&self, request: &ExploreRequest) -> Result<(), ExploreError> {
        if request.roots.is_empty() {
            return Err(ExploreError::NoRoots);
        }
        if request.roots.len() > self.limits.max_roots {
            return Err(ExploreError::TooManyRoots(self.limits.max_roots));
        }
        if let Some(root) = request
            .roots
            .iter()
            .find(|root| root.depth > self.limits.max_depth)
        {
            let _ = root;
            return Err(ExploreError::TooDeep(self.limits.max_depth));
        }
        if request.span.block_count() > self.limits.max_blocks {
            return Err(ExploreError::SpanTooWide(self.limits.max_blocks));
        }

        Ok(())
    }

    async fn fill(&self, span: BlockRange) -> Result<(), io::Error> {
        tracing::info!(
            "asking the rpc for blocks {}..={}",
            span.from_block(),
            span.to_block()
        );

        let mut stream = self.source.txs(span.from_block(), span.to_block()).await;
        let mut stumble = None;

        while let Some(item) = stream.next().await {
            if let Err(error) = item {
                tracing::error!(
                    "blocks {}..={} stumbled: {error}",
                    span.from_block(),
                    span.to_block()
                );
                if stumble.is_none() {
                    stumble = Some(error);
                }
            }
        }

        match stumble {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    async fn walk(&self, request: &ExploreRequest) -> Result<AddressGraph, io::Error> {
        let mut seen: HashMap<EthAddress, Walked> = HashMap::new();
        let mut roots: HashSet<EthAddress> = HashSet::new();

        for root in &request.roots {
            roots.insert(root.address);
            let walked = seen.entry(root.address).or_insert(Walked {
                depth: 0,
                budget: 0,
            });
            walked.budget = walked.budget.max(root.depth);
        }

        let mut edges: HashMap<InteractionId, InteractionEdge> = HashMap::new();
        let mut expanded: HashSet<EthAddress> = HashSet::new();
        let mut frontier: Vec<EthAddress> = roots.iter().copied().collect();
        let mut truncated = false;

        while !frontier.is_empty() {
            let asking: Vec<EthAddress> = frontier
                .drain(..)
                .filter(|address| {
                    !expanded.contains(address)
                        && seen.get(address).is_some_and(|walked| walked.budget > 0)
                })
                .collect();

            if asking.is_empty() {
                break;
            }

            let asked: HashSet<EthAddress> = asking.iter().copied().collect();
            expanded.extend(asked.iter().copied());

            let mut found: HashSet<EthAddress> = HashSet::new();

            for chunk in asking.chunks(ASKING_CHUNK) {
                let txs = self.index.txs_touching(chunk, request.span).await?;

                for (meta, slot, interaction) in self.classify(txs).await {
                    let Some((from, to)) = interaction.endpoints() else {
                        continue;
                    };
                    let (from, to) = (*from, *to);

                    let fresh = match (asked.contains(&from), asked.contains(&to)) {
                        (false, false) => continue,
                        (true, true) => None,
                        (true, false) => Some(to),
                        (false, true) => Some(from),
                    };

                    let id = InteractionId::new(*meta.tx_hash(), slot);
                    if !edges.contains_key(&id) && edges.len() >= self.limits.max_edges {
                        truncated = true;
                        continue;
                    }

                    if let Some(address) = fresh {
                        let mut budget = 0;
                        let mut depth = u32::MAX;

                        for side in [&from, &to] {
                            if !asked.contains(side) {
                                continue;
                            }
                            if let Some(walked) = seen.get(side) {
                                budget = budget.max(walked.budget);
                                depth = depth.min(walked.depth);
                            }
                        }

                        let budget = budget.saturating_sub(1);
                        let depth = depth.saturating_add(1);
                        let room = seen.len() < self.limits.max_nodes;

                        match seen.entry(address) {
                            Entry::Occupied(mut slot) => {
                                let walked = slot.get_mut();
                                walked.depth = walked.depth.min(depth);

                                if budget > walked.budget {
                                    walked.budget = budget;
                                    if budget > 0 {
                                        found.insert(address);
                                    }
                                }
                            }
                            Entry::Vacant(slot) => {
                                if !room {
                                    truncated = true;
                                    continue;
                                }

                                slot.insert(Walked { depth, budget });
                                if budget > 0 {
                                    found.insert(address);
                                }
                            }
                        }
                    }

                    edges.insert(id, InteractionEdge::new(meta, slot, interaction));
                }
            }

            frontier.extend(found);
        }

        let mut nodes: Vec<GraphNode> = seen
            .into_iter()
            .map(|(address, walked)| GraphNode {
                address,
                depth: walked.depth,
                root: roots.contains(&address),
                expanded: expanded.contains(&address),
            })
            .collect();
        nodes.sort_by(|left, right| {
            left.depth
                .cmp(&right.depth)
                .then_with(|| left.address.hex().cmp(&right.address.hex()))
        });

        let mut edges: Vec<InteractionEdge> = edges.into_values().collect();
        edges.sort_by(|left, right| {
            left.meta()
                .block_number()
                .cmp(&right.meta().block_number())
                .then_with(|| left.meta().tx_hash().cmp(right.meta().tx_hash()))
                .then_with(|| left.slot().cmp(&right.slot()))
        });

        Ok(AddressGraph {
            nodes,
            edges,
            filled: Vec::new(),
            truncated,
        })
    }

    async fn classify(&self, txs: Vec<MinedTx>) -> Vec<(TxMeta, u32, Interaction)> {
        futures::stream::iter(txs)
            .map(|mined| async move {
                let meta = TxMeta::from(mined.tx());

                match self.classificator.classificate(mined).await {
                    Ok(interactions) => (0u32..)
                        .zip(interactions)
                        .map(|(slot, interaction)| (meta.clone(), slot, interaction))
                        .collect::<Vec<_>>(),
                    Err(error) => {
                        tracing::error!("skipping {}: {error}", meta.tx_hash());
                        Vec::new()
                    }
                }
            })
            .buffer_unordered(CLASSIFY_CONCURRENCY)
            .concat()
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use alloy_primitives::{Bytes, TxHash, U256};
    use domain::eth::{EthReceipt, EthTx, InteractionKind, graph::NativeTransfer};

    use crate::{BoxStream, eth::classificator::ClassificateError};

    use super::*;

    fn address(last_byte: u8) -> EthAddress {
        EthAddress::from([last_byte; 20])
    }

    fn transfer(hash: u8, block: u64, from: u8, to: u8) -> MinedTx {
        let tx = EthTx::builder()
            .tx_hash(TxHash::with_last_byte(hash))
            .block_number(block)
            .timestamp(block * 12)
            .amount(U256::from(1_u64))
            .from(address(from))
            .to(address(to))
            .data(Bytes::new())
            .build();

        MinedTx::new(tx, EthReceipt::new(true, None, Vec::new()))
    }

    struct NativeOnly;

    #[async_trait::async_trait]
    impl EthTxClassificator for NativeOnly {
        async fn classificate(
            &self,
            mined: MinedTx,
        ) -> Result<Vec<Interaction>, ClassificateError> {
            let (tx, receipt) = mined.into_parts();
            let transfer =
                NativeTransfer::try_from(tx).map_err(|_| ClassificateError::InvariantNarushen)?;

            Ok(vec![Interaction::new(
                receipt,
                InteractionKind::NativeTransfer(transfer),
            )])
        }
    }

    #[derive(Default)]
    struct SilentSource {
        filled: Mutex<Vec<(u64, u64)>>,
    }

    #[async_trait::async_trait]
    impl EthTxSource for SilentSource {
        async fn txs(
            &self,
            lower_block: u64,
            highest_block: u64,
        ) -> BoxStream<'_, Result<MinedTx, io::Error>> {
            self.filled
                .lock()
                .unwrap()
                .push((lower_block, highest_block));

            Box::pin(async_stream::stream! {
                for tx in Vec::<MinedTx>::new() {
                    yield Ok(tx);
                }
            })
        }
    }

    struct FakeIndex {
        txs: Vec<MinedTx>,
        coverage: IndexCoverage,
        persistent: bool,
        asked: Mutex<Vec<Vec<EthAddress>>>,
    }

    impl FakeIndex {
        fn new(txs: Vec<MinedTx>, coverage: IndexCoverage) -> Self {
            Self {
                txs,
                coverage,
                persistent: true,
                asked: Mutex::default(),
            }
        }
    }

    #[async_trait::async_trait]
    impl EthTxIndex for FakeIndex {
        fn persistent(&self) -> bool {
            self.persistent
        }

        async fn coverage(&self, _span: BlockRange) -> Result<IndexCoverage, io::Error> {
            Ok(self.coverage.clone())
        }

        async fn histogram(
            &self,
            _span: BlockRange,
            _buckets: u32,
        ) -> Result<Vec<BlockBucket>, io::Error> {
            Ok(Vec::new())
        }

        async fn txs_touching(
            &self,
            addresses: &[EthAddress],
            span: BlockRange,
        ) -> Result<Vec<MinedTx>, io::Error> {
            self.asked.lock().unwrap().push(addresses.to_vec());
            let wanted: HashSet<EthAddress> = addresses.iter().copied().collect();

            Ok(self
                .txs
                .iter()
                .filter(|mined| span.contains(mined.tx().block_number()))
                .filter(|mined| mined.touches(&wanted))
                .cloned()
                .collect())
        }
    }

    fn chain() -> Vec<MinedTx> {
        vec![
            transfer(1, 10, 1, 2),
            transfer(2, 11, 2, 3),
            transfer(3, 12, 3, 4),
            transfer(4, 13, 7, 8),
        ]
    }

    fn explorer(index: Arc<FakeIndex>, source: Arc<SilentSource>) -> EthExplorer {
        EthExplorer::new(
            index,
            source,
            Arc::new(NativeOnly),
            ExploreLimits::default(),
        )
    }

    fn covered() -> IndexCoverage {
        IndexCoverage::new(vec![BlockRange::new(0, 100)], 4)
    }

    async fn graph_of(explorer: &EthExplorer, request: ExploreRequest) -> AddressGraph {
        match explorer.explore(request).await.unwrap() {
            Exploration::Graph(graph) => *graph,
            Exploration::RpcNeeded(_) => panic!("the span was covered"),
        }
    }

    fn ask(roots: Vec<GraphRoot>) -> ExploreRequest {
        ExploreRequest::new(roots, BlockRange::new(0, 100), false)
    }

    #[tokio::test]
    async fn depth_one_stops_at_the_neighbours() {
        let explorer = explorer(
            Arc::new(FakeIndex::new(chain(), covered())),
            Arc::new(SilentSource::default()),
        );

        let graph = graph_of(&explorer, ask(vec![GraphRoot::new(address(1), 1)])).await;
        let nodes: Vec<String> = graph.nodes().iter().map(|n| n.address().hex()).collect();

        assert_eq!(graph.edges().len(), 1);
        assert_eq!(nodes, vec![address(1).hex(), address(2).hex()]);
    }

    #[tokio::test]
    async fn depth_two_reaches_the_second_hop() {
        let explorer = explorer(
            Arc::new(FakeIndex::new(chain(), covered())),
            Arc::new(SilentSource::default()),
        );

        let graph = graph_of(&explorer, ask(vec![GraphRoot::new(address(1), 2)])).await;

        assert_eq!(graph.edges().len(), 2);
        assert_eq!(graph.nodes().len(), 3);
        assert_eq!(
            graph
                .nodes()
                .iter()
                .find(|node| node.address() == &address(3))
                .map(GraphNode::depth),
            Some(2)
        );
    }

    #[tokio::test]
    async fn every_root_keeps_its_own_depth() {
        let explorer = explorer(
            Arc::new(FakeIndex::new(chain(), covered())),
            Arc::new(SilentSource::default()),
        );

        let graph = graph_of(
            &explorer,
            ask(vec![
                GraphRoot::new(address(1), 1),
                GraphRoot::new(address(7), 0),
            ]),
        )
        .await;

        let addresses: HashSet<String> = graph.nodes().iter().map(|n| n.address().hex()).collect();

        assert_eq!(graph.edges().len(), 1);
        assert!(addresses.contains(&address(7).hex()));
        assert!(!addresses.contains(&address(8).hex()));
    }

    #[tokio::test]
    async fn a_root_with_no_traffic_still_lands_on_the_canvas() {
        let explorer = explorer(
            Arc::new(FakeIndex::new(chain(), covered())),
            Arc::new(SilentSource::default()),
        );

        let graph = graph_of(&explorer, ask(vec![GraphRoot::new(address(200), 2)])).await;

        assert_eq!(graph.nodes().len(), 1);
        assert!(graph.edges().is_empty());
        assert!(graph.nodes()[0].root());
    }

    #[tokio::test]
    async fn an_unwalked_node_is_marked_unexpanded() {
        let explorer = explorer(
            Arc::new(FakeIndex::new(chain(), covered())),
            Arc::new(SilentSource::default()),
        );

        let graph = graph_of(&explorer, ask(vec![GraphRoot::new(address(1), 1)])).await;
        let leaf = graph
            .nodes()
            .iter()
            .find(|node| node.address() == &address(2))
            .unwrap();

        assert!(!leaf.expanded());
        assert!(graph.nodes()[0].expanded());
    }

    #[tokio::test]
    async fn an_uncovered_span_asks_before_touching_the_rpc() {
        let index = Arc::new(FakeIndex::new(
            chain(),
            IndexCoverage::new(vec![BlockRange::new(0, 40)], 4),
        ));
        let source = Arc::new(SilentSource::default());
        let explorer = explorer(index, source.clone());

        let answer = explorer
            .explore(ExploreRequest::new(
                vec![GraphRoot::new(address(1), 1)],
                BlockRange::new(20, 60),
                false,
            ))
            .await
            .unwrap();

        match answer {
            Exploration::RpcNeeded(plan) => {
                assert_eq!(plan.missing(), [BlockRange::new(41, 60)]);
                assert_eq!(plan.missing_blocks(), 20);
                assert_eq!(plan.indexed_blocks(), 21);
            }
            Exploration::Graph(_) => panic!("the span was not covered"),
        }

        assert!(source.filled.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_confirmed_span_fills_only_the_holes() {
        let index = Arc::new(FakeIndex::new(
            chain(),
            IndexCoverage::new(vec![BlockRange::new(0, 40)], 4),
        ));
        let source = Arc::new(SilentSource::default());
        let explorer = explorer(index, source.clone());

        let graph = graph_of(
            &explorer,
            ExploreRequest::new(
                vec![GraphRoot::new(address(1), 1)],
                BlockRange::new(20, 60),
                true,
            ),
        )
        .await;

        assert_eq!(source.filled.lock().unwrap().clone(), vec![(41, 60)]);
        assert_eq!(graph.filled(), [BlockRange::new(41, 60)]);
    }

    #[tokio::test]
    async fn a_span_wider_than_the_limit_is_turned_away() {
        let explorer = explorer(
            Arc::new(FakeIndex::new(chain(), covered())),
            Arc::new(SilentSource::default()),
        );

        let error = explorer
            .explore(ExploreRequest::new(
                vec![GraphRoot::new(address(1), 1)],
                BlockRange::new(0, 1_000_000),
                true,
            ))
            .await
            .unwrap_err();

        assert!(matches!(error, ExploreError::SpanTooWide(_)));
    }

    #[tokio::test]
    async fn too_deep_a_root_is_turned_away() {
        let explorer = explorer(
            Arc::new(FakeIndex::new(chain(), covered())),
            Arc::new(SilentSource::default()),
        );

        let error = explorer
            .explore(ask(vec![GraphRoot::new(address(1), 99)]))
            .await
            .unwrap_err();

        assert!(matches!(error, ExploreError::TooDeep(_)));
    }

    #[tokio::test]
    async fn the_node_cap_cuts_the_walk_short() {
        let index = Arc::new(FakeIndex::new(chain(), covered()));
        let explorer = EthExplorer::new(
            index,
            Arc::new(SilentSource::default()),
            Arc::new(NativeOnly),
            ExploreLimits {
                max_nodes: 2,
                ..ExploreLimits::default()
            },
        );

        let graph = graph_of(&explorer, ask(vec![GraphRoot::new(address(1), 3)])).await;

        assert!(graph.truncated());
        assert_eq!(graph.nodes().len(), 2);
    }
}
