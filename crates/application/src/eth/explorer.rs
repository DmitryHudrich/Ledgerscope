use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    fmt, io,
    sync::Arc,
};

use futures::StreamExt;

use alloy_primitives::TxHash;

use domain::eth::{
    Actor, ActorHint, BlockBucket, BlockRange, EthAddress, IndexCoverage, Interaction,
    InteractionEdge, InteractionId, LowLevelActor, LowLevelInteraction, MinedTx, TxMeta,
};

use crate::eth::{
    LabelProvider,
    classificator::EthTxClassificator,
    lowlevel::{LowLevelGraph, LowLevelNode},
    ports::{ActorResolver, EthTxIndex, EthTxSource},
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

#[derive(Clone, Debug)]
pub struct GraphNode {
    actor: Actor,
    depth: u32,
    root: bool,
    expanded: bool,
}

impl GraphNode {
    pub fn actor(&self) -> &Actor {
        &self.actor
    }

    pub fn address(&self) -> &EthAddress {
        self.actor.address()
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
    actors: Vec<Actor>,
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

    pub fn actors(&self) -> &[Actor] {
        &self.actors
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
pub enum Exploration<G> {
    Graph(Box<G>),
    RpcNeeded(RpcPlan),
}

enum Ready {
    Walk(Vec<BlockRange>),
    Confirm(RpcPlan),
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
    label_provider: Arc<dyn LabelProvider>,
    actors: Arc<dyn ActorResolver>,
    limits: ExploreLimits,
}

impl EthExplorer {
    pub fn new(
        index: Arc<dyn EthTxIndex>,
        source: Arc<dyn EthTxSource>,
        classificator: Arc<dyn EthTxClassificator>,
        actors: Arc<dyn ActorResolver>,
        label_provider: Arc<dyn LabelProvider>,
        limits: ExploreLimits,
    ) -> Self {
        Self {
            index,
            source,
            classificator,
            actors,
            limits,
            label_provider,
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

    async fn prepare(&self, request: &ExploreRequest) -> Result<Ready, ExploreError> {
        self.weigh(request)?;

        let coverage = self
            .index
            .coverage(request.span)
            .await
            .map_err(ExploreError::Io)?;
        let missing = coverage.gaps(request.span);

        if !missing.is_empty() && !request.confirm_rpc {
            return Ok(Ready::Confirm(RpcPlan::new(
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

        Ok(Ready::Walk(missing))
    }

    pub async fn explore(
        &self,
        request: ExploreRequest,
    ) -> Result<Exploration<AddressGraph>, ExploreError> {
        let missing = match self.prepare(&request).await? {
            Ready::Confirm(plan) => return Ok(Exploration::RpcNeeded(plan)),
            Ready::Walk(missing) => missing,
        };

        let mut graph = self.walk(&request).await.map_err(ExploreError::Io)?;
        graph.filled = missing;

        Ok(Exploration::Graph(Box::new(graph)))
    }

    pub async fn explore_low_level(
        &self,
        request: ExploreRequest,
    ) -> Result<Exploration<LowLevelGraph>, ExploreError> {
        let missing = match self.prepare(&request).await? {
            Ready::Confirm(plan) => return Ok(Exploration::RpcNeeded(plan)),
            Ready::Walk(missing) => missing,
        };

        let mut graph = self
            .walk_low_level(&request)
            .await
            .map_err(ExploreError::Io)?;
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

    fn note(
        &self,
        seen: &mut HashMap<EthAddress, Walked>,
        found: &mut HashSet<EthAddress>,
        asked: &HashSet<EthAddress>,
        edge: (EthAddress, EthAddress),
        fresh: EthAddress,
    ) -> bool {
        let mut budget = 0;
        let mut depth = u32::MAX;

        for side in [&edge.0, &edge.1] {
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

        match seen.entry(fresh) {
            Entry::Occupied(mut slot) => {
                let walked = slot.get_mut();
                walked.depth = walked.depth.min(depth);

                if budget > walked.budget {
                    walked.budget = budget;
                    if budget > 0 {
                        found.insert(fresh);
                    }
                }

                true
            }
            Entry::Vacant(slot) => {
                if !room {
                    return false;
                }

                slot.insert(Walked { depth, budget });
                if budget > 0 {
                    found.insert(fresh);
                }

                true
            }
        }
    }

    fn start(request: &ExploreRequest) -> (HashMap<EthAddress, Walked>, HashSet<EthAddress>) {
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

        (seen, roots)
    }

    async fn walk_low_level(&self, request: &ExploreRequest) -> Result<LowLevelGraph, io::Error> {
        let (mut seen, roots) = Self::start(request);

        let mut edges: HashMap<TxHash, LowLevelInteraction> = HashMap::new();
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
                for mined in self.index.txs_touching(chunk, request.span).await? {
                    let Ok(interaction) = LowLevelInteraction::try_from(&mined) else {
                        continue;
                    };

                    let (from, to) = interaction.endpoints();
                    let (from, to) = (*from, *to);

                    let fresh = match (asked.contains(&from), asked.contains(&to)) {
                        (false, false) => continue,
                        (true, true) => None,
                        (true, false) => Some(to),
                        (false, true) => Some(from),
                    };

                    let id = *interaction.meta().tx_hash();
                    if !edges.contains_key(&id) && edges.len() >= self.limits.max_edges {
                        truncated = true;
                        continue;
                    }

                    if let Some(address) = fresh
                        && !self.note(&mut seen, &mut found, &asked, (from, to), address)
                    {
                        truncated = true;
                    }

                    edges.insert(id, interaction);
                }
            }

            frontier.extend(found);
        }

        let mut labeled = self
            .label_provider
            .fetch_labels_for(seen.keys().copied().collect::<Vec<_>>().as_slice())
            .await?;

        let mut actors: Vec<LowLevelNode> = seen
            .into_iter()
            .map(|(address, walked)| {
                LowLevelNode::new(
                    LowLevelActor::new(
                        address,
                        labeled.remove(&address).expect("Must be set above"),
                    ),
                    walked.depth,
                    roots.contains(&address),
                    expanded.contains(&address),
                )
            })
            .collect();
        actors.sort_by(|left, right| {
            left.depth()
                .cmp(&right.depth())
                .then_with(|| left.address().hex().cmp(&right.address().hex()))
        });

        let mut interactions: Vec<LowLevelInteraction> = edges.into_values().collect();
        interactions.sort_by(|left, right| {
            left.meta()
                .block_number()
                .cmp(&right.meta().block_number())
                .then_with(|| left.meta().tx_hash().cmp(right.meta().tx_hash()))
        });

        Ok(LowLevelGraph {
            actors,
            interactions,
            filled: Vec::new(),
            truncated,
        })
    }

    async fn walk(&self, request: &ExploreRequest) -> Result<AddressGraph, io::Error> {
        let (mut seen, roots) = Self::start(request);

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

                    if let Some(address) = fresh
                        && !self.note(&mut seen, &mut found, &asked, (from, to), address)
                    {
                        truncated = true;
                    }

                    edges.insert(id, InteractionEdge::new(meta, slot, interaction));
                }
            }

            frontier.extend(found);
        }

        let mut wanted: HashMap<EthAddress, Option<ActorHint>> = HashMap::new();

        for edge in edges.values() {
            for (address, hint) in edge.actor_hints() {
                match wanted.entry(address) {
                    Entry::Occupied(mut slot) => {
                        let best = slot.get_mut();
                        if best.is_none_or(|known| hint.outranks(known)) {
                            *best = Some(hint);
                        }
                    }
                    Entry::Vacant(slot) => {
                        slot.insert(Some(hint));
                    }
                }
            }
        }

        for address in seen.keys() {
            wanted.entry(*address).or_insert(None);
        }

        let mut resolved = self.actors.resolve(wanted).await;

        let mut nodes: Vec<GraphNode> = seen
            .into_iter()
            .map(|(address, walked)| GraphNode {
                actor: resolved
                    .get(&address)
                    .cloned()
                    .unwrap_or_else(|| Actor::unknown(address)),
                depth: walked.depth,
                root: roots.contains(&address),
                expanded: expanded.contains(&address),
            })
            .collect();
        nodes.sort_by(|left, right| {
            left.depth
                .cmp(&right.depth)
                .then_with(|| left.address().hex().cmp(&right.address().hex()))
        });

        let mut actors: Vec<Actor> = resolved.drain().map(|(_, actor)| actor).collect();
        actors.sort_by_key(|actor| actor.address().hex());

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
            actors,
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
