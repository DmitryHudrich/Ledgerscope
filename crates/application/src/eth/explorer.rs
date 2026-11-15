use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    fmt, io,
    sync::Arc,
};

use futures::StreamExt;

use alloy_primitives::TxHash;

use domain::eth::{
    Actor, ActorHint, BlockBucket, BlockRange, EthAddress, IndexCoverage, Interaction,
    InteractionEdge, InteractionId, LowLevelActor, LowLevelInteraction, MinedTx, Tag, TxMeta,
};

use crate::eth::{
    LabelProvider,
    build::{BuildObserver, BuildRun, BuildStage, NoBuilds},
    classificator::EthTxClassificator,
    lowlevel::{LowLevelGraph, LowLevelNode},
    ports::{ActorResolver, EthTxIndex, EthTxSource},
    scan::{NoScans, ScanObserver, ScanRun},
    tags::TagEngine,
};

const ASKING_CHUNK: usize = 128;
const FILL_WINDOW: u64 = 24;
const CLASSIFY_CONCURRENCY: usize = 20;
const PROGRESS_STEP: u64 = 256;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExploreScope {
    #[default]
    Roots,
    Blocks,
}

#[derive(Clone, Debug)]
pub struct ExploreRequest {
    roots: Vec<GraphRoot>,
    spans: Vec<BlockRange>,
    confirm_rpc: bool,
    scope: ExploreScope,
}

impl ExploreRequest {
    pub fn new(roots: Vec<GraphRoot>, span: BlockRange, confirm_rpc: bool) -> Self {
        Self::with_spans(roots, vec![span], confirm_rpc)
    }

    pub fn with_spans(roots: Vec<GraphRoot>, spans: Vec<BlockRange>, confirm_rpc: bool) -> Self {
        Self {
            roots,
            spans,
            confirm_rpc,
            scope: ExploreScope::Roots,
        }
    }

    pub fn blocks(spans: Vec<BlockRange>, confirm_rpc: bool) -> Self {
        Self {
            roots: Vec::new(),
            spans,
            confirm_rpc,
            scope: ExploreScope::Blocks,
        }
    }

    pub fn scope(&self) -> ExploreScope {
        self.scope
    }

    pub fn roots(&self) -> &[GraphRoot] {
        &self.roots
    }

    pub fn spans(&self) -> &[BlockRange] {
        &self.spans
    }

    pub fn span(&self) -> BlockRange {
        let from = self
            .spans
            .iter()
            .map(BlockRange::from_block)
            .min()
            .unwrap_or(0);
        let to = self
            .spans
            .iter()
            .map(BlockRange::to_block)
            .max()
            .unwrap_or(0);
        BlockRange::new(from, to)
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
    tags: Vec<Tag>,
    filled: Vec<BlockRange>,
    truncated: bool,
}

impl AddressGraph {
    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

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
    scans: Arc<dyn ScanObserver>,
    builds: Arc<dyn BuildObserver>,
    tagger: Arc<TagEngine>,
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
            scans: Arc::new(NoScans),
            builds: Arc::new(NoBuilds),
            tagger: Arc::new(TagEngine::standard()),
        }
    }

    pub fn with_scans(self, scans: Arc<dyn ScanObserver>) -> Self {
        Self { scans, ..self }
    }

    pub fn with_builds(self, builds: Arc<dyn BuildObserver>) -> Self {
        Self { builds, ..self }
    }

    pub fn with_tagger(self, tagger: Arc<TagEngine>) -> Self {
        Self { tagger, ..self }
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

    async fn prepare(
        &self,
        request: &ExploreRequest,
        run: &BuildRun<'_>,
    ) -> Result<Ready, ExploreError> {
        let mut missing: Vec<BlockRange> = Vec::new();
        let mut indexed = 0u64;
        run.stage(BuildStage::Coverage, request.spans().len() as u64);
        for span in request.spans() {
            let coverage = self.index.coverage(*span).await.map_err(ExploreError::Io)?;
            missing.extend(coverage.gaps(*span));
            indexed += coverage.indexed_within(*span);
            run.advance(1, 0);
        }

        if !missing.is_empty() && !request.confirm_rpc {
            return Ok(Ready::Confirm(RpcPlan::new(
                missing.clone(),
                indexed,
                missing.iter().map(BlockRange::block_count).sum(),
            )));
        }

        if !missing.is_empty() && self.index.persistent() {
            let scan = ScanRun::begin(self.scans.as_ref(), &missing);
            let mut filled = Ok(());
            for gap in &missing {
                filled = self.fill(*gap, scan.id()).await;
                if filled.is_err() {
                    break;
                }
            }
            scan.finish(filled.as_ref().err().map(ToString::to_string));
            filled.map_err(ExploreError::Io)?;
        }

        Ok(Ready::Walk(missing))
    }

    pub async fn explore(
        &self,
        request: ExploreRequest,
    ) -> Result<Exploration<AddressGraph>, ExploreError> {
        self.weigh(&request)?;
        let run = BuildRun::begin(self.builds.as_ref(), request.spans());
        let built = self.build(&request, &run).await;
        run.finish(built.as_ref().err().map(ToString::to_string));
        built
    }

    async fn build(
        &self,
        request: &ExploreRequest,
        run: &BuildRun<'_>,
    ) -> Result<Exploration<AddressGraph>, ExploreError> {
        let missing = match self.prepare(request, run).await? {
            Ready::Confirm(plan) => return Ok(Exploration::RpcNeeded(plan)),
            Ready::Walk(missing) => missing,
        };

        let mut graph = match request.scope {
            ExploreScope::Roots => self.walk(request, run).await,
            ExploreScope::Blocks => self.sweep(request, run).await,
        }
        .map_err(ExploreError::Io)?;
        graph.filled = missing;
        run.stage(BuildStage::Tags, 1);
        graph.tags = self.tagger.tag(&graph.edges, &graph.actors);
        run.advance(1, 0);

        Ok(Exploration::Graph(Box::new(graph)))
    }

    pub async fn explore_low_level(
        &self,
        request: ExploreRequest,
    ) -> Result<Exploration<LowLevelGraph>, ExploreError> {
        self.weigh(&request)?;
        let run = BuildRun::begin(&NoBuilds, request.spans());
        let missing = match self.prepare(&request, &run).await? {
            Ready::Confirm(plan) => return Ok(Exploration::RpcNeeded(plan)),
            Ready::Walk(missing) => missing,
        };

        let mut graph = match request.scope {
            ExploreScope::Roots => self.walk_low_level(&request).await,
            ExploreScope::Blocks => self.sweep_low_level(&request, &run).await,
        }
        .map_err(ExploreError::Io)?;
        graph.filled = missing;

        Ok(Exploration::Graph(Box::new(graph)))
    }

    fn weigh(&self, request: &ExploreRequest) -> Result<(), ExploreError> {
        if request.scope == ExploreScope::Roots && request.roots.is_empty() {
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
        let total: u64 = request.spans.iter().map(BlockRange::block_count).sum();
        if total > self.limits.max_blocks {
            return Err(ExploreError::SpanTooWide(self.limits.max_blocks));
        }

        Ok(())
    }

    async fn fill(&self, span: BlockRange, job: u64) -> Result<(), io::Error> {
        tracing::info!(
            "asking the rpc for blocks {}..={}",
            span.from_block(),
            span.to_block()
        );

        let mut from = span.from_block();
        loop {
            let to = from.saturating_add(FILL_WINDOW - 1).min(span.to_block());
            let mut stream = self.source.txs(from, to).await;
            let mut stumble = None;
            let mut txs = 0;

            while let Some(item) = stream.next().await {
                match item {
                    Ok(_) => txs += 1,
                    Err(error) => {
                        tracing::error!("blocks {from}..={to} stumbled: {error}");
                        if stumble.is_none() {
                            stumble = Some(error);
                        }
                    }
                }
            }

            if let Some(error) = stumble {
                return Err(error);
            }
            self.scans.advance(job, to - from + 1, txs);

            if to >= span.to_block() {
                return Ok(());
            }
            from = to + 1;
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
                let mut mined_txs = Vec::new();
                for span in request.spans() {
                    mined_txs.extend(self.index.txs_touching(chunk, *span).await?);
                }
                for mined in mined_txs {
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

        self.assemble_low_level(
            edges.into_values().collect(),
            seen,
            &roots,
            &expanded,
            truncated,
        )
        .await
    }

    async fn sweep_low_level(
        &self,
        request: &ExploreRequest,
        run: &BuildRun<'_>,
    ) -> Result<LowLevelGraph, io::Error> {
        let (txs, mut truncated) = self.within(request, run).await?;
        let mut seen: HashMap<EthAddress, Walked> = HashMap::new();
        let mut interactions = Vec::new();

        for mined in txs {
            let Ok(interaction) = LowLevelInteraction::try_from(&mined) else {
                continue;
            };
            let (from, to) = interaction.endpoints();
            if !self.admit(&mut seen, *from, *to) {
                truncated = true;
                continue;
            }
            interactions.push(interaction);
        }

        self.assemble_low_level(
            interactions,
            seen,
            &HashSet::new(),
            &HashSet::new(),
            truncated,
        )
        .await
    }

    async fn within(
        &self,
        request: &ExploreRequest,
        run: &BuildRun<'_>,
    ) -> Result<(Vec<MinedTx>, bool), io::Error> {
        let mut txs = Vec::new();
        let mut truncated = false;
        run.stage(BuildStage::Index, request.spans().len() as u64);

        for span in request.spans() {
            let room = self.limits.max_edges.saturating_sub(txs.len());
            if room == 0 {
                truncated = true;
                break;
            }
            let batch = self.index.txs_within(*span, room + 1).await?;
            truncated |= batch.len() > room;
            run.advance(1, batch.len().min(room) as u64);
            txs.extend(batch.into_iter().take(room));
        }

        Ok((txs, truncated))
    }

    fn admit(
        &self,
        seen: &mut HashMap<EthAddress, Walked>,
        from: EthAddress,
        to: EthAddress,
    ) -> bool {
        let fresh = [from, to]
            .into_iter()
            .filter(|address| !seen.contains_key(address))
            .collect::<HashSet<_>>();
        if seen.len() + fresh.len() > self.limits.max_nodes {
            return false;
        }
        for address in fresh {
            seen.insert(
                address,
                Walked {
                    depth: 0,
                    budget: 0,
                },
            );
        }
        true
    }

    async fn assemble_low_level(
        &self,
        interactions: Vec<LowLevelInteraction>,
        seen: HashMap<EthAddress, Walked>,
        roots: &HashSet<EthAddress>,
        expanded: &HashSet<EthAddress>,
        truncated: bool,
    ) -> Result<LowLevelGraph, io::Error> {
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

        let mut interactions = interactions;
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

    async fn walk(
        &self,
        request: &ExploreRequest,
        run: &BuildRun<'_>,
    ) -> Result<AddressGraph, io::Error> {
        let (mut seen, roots) = Self::start(request);
        run.stage(BuildStage::Walk, 0);

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
            run.grow(asking.len() as u64);

            let mut found: HashSet<EthAddress> = HashSet::new();

            for chunk in asking.chunks(ASKING_CHUNK) {
                let mut txs = Vec::new();
                for span in request.spans() {
                    txs.extend(self.index.txs_touching(chunk, *span).await?);
                }
                run.advance(chunk.len() as u64, txs.len() as u64);

                for (meta, slot, interaction) in self.classify(txs, None).await {
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

        self.assemble(
            edges.into_values().collect(),
            seen,
            &roots,
            &expanded,
            truncated,
            run,
        )
        .await
    }

    async fn sweep(
        &self,
        request: &ExploreRequest,
        run: &BuildRun<'_>,
    ) -> Result<AddressGraph, io::Error> {
        let (txs, mut truncated) = self.within(request, run).await?;
        let mut seen: HashMap<EthAddress, Walked> = HashMap::new();
        let mut edges = Vec::new();

        run.stage(BuildStage::Classify, txs.len() as u64);
        for (meta, slot, interaction) in self.classify(txs, Some(run)).await {
            let Some((from, to)) = interaction.endpoints() else {
                continue;
            };
            if edges.len() >= self.limits.max_edges || !self.admit(&mut seen, *from, *to) {
                truncated = true;
                continue;
            }
            edges.push(InteractionEdge::new(meta, slot, interaction));
        }

        self.assemble(
            edges,
            seen,
            &HashSet::new(),
            &HashSet::new(),
            truncated,
            run,
        )
        .await
    }

    async fn assemble(
        &self,
        edges: Vec<InteractionEdge>,
        seen: HashMap<EthAddress, Walked>,
        roots: &HashSet<EthAddress>,
        expanded: &HashSet<EthAddress>,
        truncated: bool,
        run: &BuildRun<'_>,
    ) -> Result<AddressGraph, io::Error> {
        let mut wanted: HashMap<EthAddress, Option<ActorHint>> = HashMap::new();

        for edge in &edges {
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

        let asked = wanted.len() as u64;
        run.stage(BuildStage::Actors, asked);
        let mut resolved = self.actors.resolve(wanted).await;
        run.advance(asked, 0);

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

        let mut edges = edges;
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
            tags: Vec::new(),
            filled: Vec::new(),
            truncated,
        })
    }

    async fn classify(
        &self,
        txs: Vec<MinedTx>,
        run: Option<&BuildRun<'_>>,
    ) -> Vec<(TxMeta, u32, Interaction)> {
        let mut classified = futures::stream::iter(txs)
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
            .buffer_unordered(CLASSIFY_CONCURRENCY);

        let mut found = Vec::new();
        let mut pending = 0;
        while let Some(interactions) = classified.next().await {
            found.extend(interactions);
            pending += 1;
            if pending == PROGRESS_STEP
                && let Some(run) = run
            {
                run.advance(pending, 0);
                pending = 0;
            }
        }
        if let Some(run) = run {
            run.advance(pending, 0);
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Bytes, U256};
    use domain::eth::{AddressLabel, EthReceipt, EthTx};

    use crate::{
        BoxStream,
        eth::{FetchingTxIndex, classificator::FulliestEthTxClassificator},
    };

    use super::*;

    fn address(last_byte: u8) -> EthAddress {
        EthAddress::from([last_byte; 20])
    }

    fn transfer(block: u64, from: u8, to: u8) -> MinedTx {
        let tx = EthTx::builder()
            .tx_hash(TxHash::with_last_byte(from))
            .block_number(block)
            .timestamp(block * 12)
            .amount(U256::from(1_u64))
            .from(address(from))
            .to(address(to))
            .data(Bytes::new())
            .build();

        MinedTx::new(tx, EthReceipt::new(true, None, Vec::new()))
    }

    struct Blocks;

    #[async_trait::async_trait]
    impl EthTxSource for Blocks {
        async fn txs(
            &self,
            _lower_block: u64,
            _highest_block: u64,
        ) -> BoxStream<'_, Result<MinedTx, io::Error>> {
            Box::pin(async_stream::stream! {
                yield Ok(transfer(10, 1, 2));
                yield Ok(transfer(10, 3, 4));
                yield Ok(transfer(11, 5, 6));
            })
        }
    }

    struct Unknown;

    #[async_trait::async_trait]
    impl ActorResolver for Unknown {
        async fn resolve(
            &self,
            _wanted: HashMap<EthAddress, Option<ActorHint>>,
        ) -> HashMap<EthAddress, Actor> {
            HashMap::new()
        }
    }

    struct Unlabeled;

    #[async_trait::async_trait]
    impl LabelProvider for Unlabeled {
        fn source(&self) -> &str {
            "nothing"
        }

        async fn fetch_labels_for(
            &self,
            addresses: &[EthAddress],
        ) -> Result<HashMap<EthAddress, Vec<AddressLabel>>, io::Error> {
            Ok(addresses
                .iter()
                .map(|address| (*address, Vec::new()))
                .collect())
        }
    }

    fn explorer(limits: ExploreLimits) -> EthExplorer {
        let source = Arc::new(Blocks);
        EthExplorer::new(
            Arc::new(FetchingTxIndex::new(source.clone())),
            source,
            Arc::new(FulliestEthTxClassificator::new()),
            Arc::new(Unknown),
            Arc::new(Unlabeled),
            limits,
        )
    }

    fn graph(exploration: Exploration<AddressGraph>) -> AddressGraph {
        match exploration {
            Exploration::Graph(graph) => *graph,
            Exploration::RpcNeeded(_) => panic!("the fetching index needs no confirmation"),
        }
    }

    #[tokio::test]
    async fn a_block_sweep_takes_every_tx_without_roots() {
        let request = ExploreRequest::blocks(vec![BlockRange::new(10, 11)], true);

        let swept = graph(
            explorer(ExploreLimits::default())
                .explore(request)
                .await
                .unwrap(),
        );

        assert_eq!(swept.edges().len(), 3);
        assert_eq!(swept.nodes().len(), 6);
        assert!(swept.nodes().iter().all(|node| !node.root()));
        assert!(!swept.truncated());
    }

    #[tokio::test]
    async fn a_block_sweep_stops_at_the_node_limit() {
        let limits = ExploreLimits {
            max_nodes: 4,
            ..ExploreLimits::default()
        };
        let request = ExploreRequest::blocks(vec![BlockRange::new(10, 11)], true);

        let swept = graph(explorer(limits).explore(request).await.unwrap());

        assert_eq!(swept.nodes().len(), 4);
        assert_eq!(swept.edges().len(), 2);
        assert!(swept.truncated());
    }

    #[tokio::test]
    async fn a_block_sweep_stops_at_the_edge_limit() {
        let limits = ExploreLimits {
            max_edges: 2,
            ..ExploreLimits::default()
        };
        let request = ExploreRequest::blocks(vec![BlockRange::new(10, 11)], true);

        let swept = graph(explorer(limits).explore(request).await.unwrap());

        assert_eq!(swept.edges().len(), 2);
        assert!(swept.truncated());
    }

    #[tokio::test]
    async fn a_root_walk_still_needs_a_root() {
        let request = ExploreRequest::with_spans(Vec::new(), vec![BlockRange::new(10, 11)], true);

        let refused = explorer(ExploreLimits::default()).explore(request).await;

        assert!(matches!(refused, Err(ExploreError::NoRoots)));
    }
}
