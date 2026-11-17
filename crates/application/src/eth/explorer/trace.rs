use std::collections::{HashMap, HashSet};

use alloy_primitives::TxHash;

use domain::eth::{
    EthAddress, Interaction, InteractionEdge, InteractionId, InteractionKind, MinedTx,
    graph::NativeTransfer,
};

use super::{
    ASKING_CHUNK, AddressGraph, EthExplorer, Exploration, ExploreError, ExploreRequest, Ready,
    Walked,
};
use crate::eth::{
    build::{BuildRun, BuildStage},
    taint::{self, SYNTHETIC_SLOT, TaintPolicy, TaintScope, TaintTrace, TaintedMove, TxFlow},
};

#[derive(Clone, Debug, Default)]
pub struct TraceRequest {
    pub policy: TaintPolicy,
    pub stops: Vec<EthAddress>,
    pub wrapped_native: Option<EthAddress>,
}

#[derive(Debug)]
pub struct TaintGraph {
    graph: AddressGraph,
    trace: TaintTrace,
    seeds: Vec<EthAddress>,
    sinks: Vec<EthAddress>,
}

impl TaintGraph {
    pub fn graph(&self) -> &AddressGraph {
        &self.graph
    }

    pub fn trace(&self) -> &TaintTrace {
        &self.trace
    }

    pub fn seeds(&self) -> &[EthAddress] {
        &self.seeds
    }

    pub fn sinks(&self) -> &[EthAddress] {
        &self.sinks
    }
}

struct Known {
    mined: MinedTx,
    interactions: Vec<Interaction>,
    flow: Option<TxFlow>,
}

impl EthExplorer {
    pub async fn trace(
        &self,
        request: ExploreRequest,
        taint: TraceRequest,
    ) -> Result<Exploration<TaintGraph>, ExploreError> {
        if request.roots().is_empty() {
            return Err(ExploreError::NoRoots);
        }
        self.weigh(&request)?;
        let run = BuildRun::begin(self.builds.as_ref(), request.spans());
        let traced = self.follow(&request, &taint, &run).await;
        run.finish(traced.as_ref().err().map(ToString::to_string));
        traced
    }

    async fn follow(
        &self,
        request: &ExploreRequest,
        taint: &TraceRequest,
        run: &BuildRun<'_>,
    ) -> Result<Exploration<TaintGraph>, ExploreError> {
        let missing = match self.prepare(request, run).await? {
            Ready::Confirm(plan) => return Ok(Exploration::RpcNeeded(plan)),
            Ready::Walk(missing) => missing,
        };

        let hops = request
            .roots()
            .iter()
            .map(|root| root.depth())
            .max()
            .unwrap_or_default();
        let seeds: HashSet<EthAddress> =
            request.roots().iter().map(|root| *root.address()).collect();
        let mut scope = TaintScope {
            seeds: seeds.clone(),
            tracked: seeds.clone(),
            sinks: taint.stops.iter().copied().collect(),
        };
        let mut depth: HashMap<EthAddress, u32> =
            seeds.iter().map(|address| (*address, 0)).collect();
        let mut fetched: HashSet<EthAddress> = HashSet::new();
        let mut known: HashMap<TxHash, Known> = HashMap::new();
        let mut truncated = false;
        let mut traced = TaintTrace::default();
        run.stage(BuildStage::Walk, 0);

        for hop in 0..=hops {
            let mut asking: Vec<EthAddress> = scope
                .tracked
                .iter()
                .filter(|address| !fetched.contains(*address) && !scope.sinks.contains(*address))
                .copied()
                .collect();
            asking.sort_by_key(EthAddress::hex);
            fetched.extend(asking.iter().copied());
            run.grow(asking.len() as u64);

            for chunk in asking.chunks(ASKING_CHUNK) {
                let room = self.limits.max_edges.saturating_sub(known.len());
                let (touched, cut) = self
                    .touching(chunk, request.spans(), self.limits.max_edges)
                    .await
                    .map_err(ExploreError::Io)?;
                truncated |= cut;

                let mut fresh: HashMap<TxHash, MinedTx> = HashMap::new();
                for mined in touched {
                    let hash = *mined.tx().tx_hash();
                    if !known.contains_key(&hash) {
                        fresh.insert(hash, mined);
                    }
                }

                if fresh.len() > room {
                    truncated = true;
                    let mut kept: Vec<TxHash> = fresh.keys().copied().collect();
                    kept.sort();
                    for hash in kept.into_iter().skip(room) {
                        fresh.remove(&hash);
                    }
                }
                run.advance(chunk.len() as u64, fresh.len() as u64);
                self.read_flows(fresh, taint, &mut known).await;
            }

            let mut flows: Vec<TxFlow> = known
                .values()
                .filter_map(|known| known.flow.clone())
                .collect();
            taint::order(&mut flows);
            traced = taint::trace(&flows, &scope, taint.policy);

            if hop == hops {
                break;
            }
            let reached: Vec<EthAddress> = traced.reached(&scope).collect();
            if reached.is_empty() {
                break;
            }

            let labels = self
                .label_provider
                .fetch_labels_for(&reached)
                .await
                .map_err(ExploreError::Io)?;
            for address in reached {
                if labels
                    .get(&address)
                    .is_some_and(|labels| !labels.is_empty())
                {
                    scope.sinks.insert(address);
                    continue;
                }
                if scope.tracked.len() >= self.limits.max_nodes {
                    truncated = true;
                    break;
                }
                scope.tracked.insert(address);
                depth.entry(address).or_insert(hop + 1);
            }
        }

        let (edges, nodes, cut) = self.draw(&traced, &known, &depth, hops);
        let mut graph = self
            .assemble(edges, nodes, &seeds, &fetched, truncated || cut, run)
            .await
            .map_err(ExploreError::Io)?;
        graph.filled = missing;
        run.stage(BuildStage::Tags, 1);
        graph.tags = self.tagger.tag(&graph.edges, &graph.actors);
        run.advance(1, 0);

        let mut seeds: Vec<EthAddress> = seeds.into_iter().collect();
        seeds.sort_by_key(EthAddress::hex);
        let mut sinks: Vec<EthAddress> = scope.sinks.into_iter().collect();
        sinks.sort_by_key(EthAddress::hex);

        Ok(Exploration::Graph(Box::new(TaintGraph {
            graph,
            trace: traced,
            seeds,
            sinks,
        })))
    }

    async fn read_flows(
        &self,
        fresh: HashMap<TxHash, MinedTx>,
        taint: &TraceRequest,
        known: &mut HashMap<TxHash, Known>,
    ) {
        let mined: Vec<MinedTx> = fresh.values().cloned().collect();
        let mut grouped: HashMap<TxHash, Vec<(u32, Interaction)>> = HashMap::new();
        for (meta, slot, interaction) in self.classify(mined, None).await {
            grouped
                .entry(*meta.tx_hash())
                .or_default()
                .push((slot, interaction));
        }

        for (hash, mined) in fresh {
            let mut slotted = grouped.remove(&hash).unwrap_or_default();
            slotted.sort_by_key(|(slot, _)| *slot);
            let interactions: Vec<Interaction> = slotted
                .into_iter()
                .map(|(_, interaction)| interaction)
                .collect();
            let flow = TxFlow::read(&mined, &interactions, taint.wrapped_native.as_ref());
            known.insert(
                hash,
                Known {
                    mined,
                    interactions,
                    flow,
                },
            );
        }
    }

    fn draw(
        &self,
        traced: &TaintTrace,
        known: &HashMap<TxHash, Known>,
        depth: &HashMap<EthAddress, u32>,
        hops: u32,
    ) -> (Vec<InteractionEdge>, HashMap<EthAddress, Walked>, bool) {
        let mut edges: HashMap<InteractionId, InteractionEdge> = HashMap::new();
        let mut nodes: HashMap<EthAddress, Walked> = depth
            .iter()
            .filter(|(_, depth)| **depth == 0)
            .map(|(address, _)| {
                (
                    *address,
                    Walked {
                        depth: 0,
                        budget: 0,
                    },
                )
            })
            .collect();
        let mut cut = false;

        let depth_of = |address: &EthAddress, from: &EthAddress| {
            depth.get(address).copied().unwrap_or_else(|| {
                depth
                    .get(from)
                    .map_or(hops.saturating_add(1), |depth| depth.saturating_add(1))
            })
        };

        let mut place = |edge: InteractionEdge, ends: [(EthAddress, u32); 2]| {
            let id = edge.id();
            if !edges.contains_key(&id) && edges.len() >= self.limits.max_edges {
                cut = true;
                return;
            }
            edges.insert(id, edge);
            for (address, depth) in ends {
                let walked = nodes.entry(address).or_insert(Walked { depth, budget: 0 });
                walked.depth = walked.depth.min(depth);
            }
        };

        for moved in traced.moves() {
            if !moved.kind().shown() {
                continue;
            }
            let Some(known) = known.get(moved.tx_hash()) else {
                continue;
            };
            let Some(edge) = edge_of(moved, known) else {
                continue;
            };
            let from = *moved.from();
            let to = *moved.to();
            let from_depth = depth_of(&from, &from);
            place(edge, [(from, from_depth), (to, depth_of(&to, &from))]);
        }

        for swap in traced.swaps() {
            let Some(known) = known.get(swap.meta.tx_hash()) else {
                continue;
            };
            let Some(interaction) = known.interactions.get(swap.slot as usize) else {
                continue;
            };
            let Some((interactor, pool)) = interaction.endpoints().map(|(a, b)| (*a, *b)) else {
                continue;
            };
            let edge = InteractionEdge::new(swap.meta.clone(), swap.slot, interaction.clone());
            let interactor_depth = depth_of(&interactor, &interactor);
            place(
                edge,
                [
                    (interactor, interactor_depth),
                    (pool, depth_of(&pool, &interactor)),
                ],
            );
        }

        (edges.into_values().collect(), nodes, cut)
    }
}

fn edge_of(moved: &TaintedMove, known: &Known) -> Option<InteractionEdge> {
    if moved.slot() < SYNTHETIC_SLOT {
        let interaction = known.interactions.get(moved.slot() as usize)?;
        return Some(InteractionEdge::new(
            moved.meta().clone(),
            moved.slot(),
            interaction.clone(),
        ));
    }

    let interaction = Interaction::new(
        known.mined.receipt().clone(),
        InteractionKind::NativeTransfer(NativeTransfer::new(
            *moved.from(),
            *moved.to(),
            moved.amount(),
        )),
    );
    Some(InteractionEdge::new(
        moved.meta().clone(),
        moved.slot(),
        interaction,
    ))
}
