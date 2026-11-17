use std::collections::{BTreeSet, HashMap, HashSet};

use alloy_primitives::{B256, TxHash, U256, U512, b256};

use domain::eth::{
    ContractAction, EthAddress, EthLog, Interaction, InteractionKind, MinedTx, TagSource, TxMeta,
};

const TRANSFER_TOPIC: B256 =
    b256!("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef");
const DEPOSIT_TOPIC: B256 =
    b256!("0xe1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c460751c2402c5c5cc9109c");
const WITHDRAWAL_TOPIC: B256 =
    b256!("0x7fcf532c15f0a6db0bd6d0e038bea71d30d808c7d98cb3bf7268a95bf5081b65");

pub const SYNTHETIC_SLOT: u32 = 1 << 31;
pub const FULL_SHARE_BPS: u64 = 10_000;
const SETTLE_PASSES: usize = 6;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Asset {
    Native,
    Token(EthAddress),
    Issued(String),
}

impl Asset {
    fn of_token(token: &EthAddress) -> Self {
        if *token == EthAddress::ZERO {
            Asset::Native
        } else {
            Asset::Token(*token)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MoveKind {
    Value,
    Transfer,
    Issued,
    Wrap,
    Unwrap,
    Payout,
}

impl MoveKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MoveKind::Value => "value",
            MoveKind::Transfer => "transfer",
            MoveKind::Issued => "issued",
            MoveKind::Wrap => "wrap",
            MoveKind::Unwrap => "unwrap",
            MoveKind::Payout => "payout",
        }
    }

    pub fn shown(self) -> bool {
        !matches!(self, MoveKind::Wrap | MoveKind::Unwrap)
    }
}

#[derive(Clone, Debug)]
pub struct Move {
    slot: u32,
    kind: MoveKind,
    asset: Asset,
    from: EthAddress,
    to: EthAddress,
    amount: U256,
}

#[derive(Clone, Debug)]
pub struct Conversion {
    slot: u32,
    pool: EthAddress,
    asset_in: Asset,
    asset_out: Asset,
    amount_out: U256,
}

#[derive(Clone, Debug)]
pub struct TxFlow {
    meta: TxMeta,
    sender: EthAddress,
    moves: Vec<Move>,
    conversions: Vec<Conversion>,
}

impl TxFlow {
    pub fn read(
        mined: &MinedTx,
        interactions: &[Interaction],
        wrapped_native: Option<&EthAddress>,
    ) -> Option<Self> {
        let tx = mined.tx();
        let receipt = mined.receipt();
        if !receipt.succeeded() || tx.action().is_some() {
            return None;
        }

        let sender = *tx.from();
        let mut flow = TxFlow {
            meta: TxMeta::from(tx),
            sender,
            moves: Vec::new(),
            conversions: Vec::new(),
        };

        if let Some(asset) = tx.asset() {
            let to = *tx.to()?;
            flow.moves.push(Move {
                slot: 0,
                kind: MoveKind::Issued,
                asset: Asset::Issued(asset.id().to_owned()),
                from: sender,
                to,
                amount: tx.amount(),
            });
            return Some(flow);
        }

        let mut synthetic = SYNTHETIC_SLOT;
        let mut next_synthetic = || {
            let slot = synthetic;
            synthetic = synthetic.saturating_add(1);
            slot
        };

        let receiver = tx.to().or(receipt.contract_address()).copied();
        if let Some(to) = receiver
            && !tx.amount().is_zero()
        {
            let plain = tx.to().is_some() && tx.data().is_empty() && to != EthAddress::ZERO;
            flow.moves.push(Move {
                slot: if plain { 0 } else { next_synthetic() },
                kind: MoveKind::Value,
                asset: Asset::Native,
                from: sender,
                to,
                amount: tx.amount(),
            });
        }

        let mut transfers = 0u32;
        for log in receipt.logs() {
            if let Some((from, to, amount)) = transfer_of(log) {
                flow.moves.push(Move {
                    slot: transfers,
                    kind: MoveKind::Transfer,
                    asset: Asset::Token(*log.address()),
                    from,
                    to,
                    amount,
                });
                transfers += 1;
                continue;
            }

            let Some(weth) = wrapped_native.filter(|weth| *weth == log.address()) else {
                continue;
            };
            if let Some((holder, amount)) = wrapping_of(log, DEPOSIT_TOPIC) {
                if tx.to() != Some(weth) {
                    flow.moves.push(Move {
                        slot: next_synthetic(),
                        kind: MoveKind::Wrap,
                        asset: Asset::Native,
                        from: holder,
                        to: *weth,
                        amount,
                    });
                }
                flow.moves.push(Move {
                    slot: next_synthetic(),
                    kind: MoveKind::Wrap,
                    asset: Asset::Token(*weth),
                    from: *weth,
                    to: holder,
                    amount,
                });
                flow.conversions.push(Conversion {
                    slot: SYNTHETIC_SLOT,
                    pool: *weth,
                    asset_in: Asset::Native,
                    asset_out: Asset::Token(*weth),
                    amount_out: amount,
                });
            } else if let Some((holder, amount)) = wrapping_of(log, WITHDRAWAL_TOPIC) {
                flow.moves.push(Move {
                    slot: next_synthetic(),
                    kind: MoveKind::Unwrap,
                    asset: Asset::Token(*weth),
                    from: holder,
                    to: *weth,
                    amount,
                });
                flow.moves.push(Move {
                    slot: next_synthetic(),
                    kind: MoveKind::Unwrap,
                    asset: Asset::Native,
                    from: *weth,
                    to: holder,
                    amount,
                });
                flow.conversions.push(Conversion {
                    slot: SYNTHETIC_SLOT,
                    pool: *weth,
                    asset_in: Asset::Token(*weth),
                    asset_out: Asset::Native,
                    amount_out: amount,
                });
                if holder != sender && tx.to() == Some(&holder) {
                    flow.moves.push(Move {
                        slot: next_synthetic(),
                        kind: MoveKind::Payout,
                        asset: Asset::Native,
                        from: holder,
                        to: sender,
                        amount,
                    });
                }
            }
        }

        for (slot, interaction) in (0u32..).zip(interactions) {
            if let InteractionKind::ContractInteraction {
                contract_interaction_type: ContractAction::Swap(swap),
                ..
            } = interaction.kind()
            {
                flow.conversions.push(Conversion {
                    slot,
                    pool: *swap.pool().address(),
                    asset_in: Asset::of_token(swap.token_in()),
                    asset_out: Asset::of_token(swap.token_out()),
                    amount_out: swap.amount_out(),
                });
            }
        }

        Some(flow)
    }

    pub fn meta(&self) -> &TxMeta {
        &self.meta
    }

    pub fn pools(&self) -> impl Iterator<Item = &EthAddress> {
        self.conversions.iter().map(|conversion| &conversion.pool)
    }
}

fn transfer_of(log: &EthLog) -> Option<(EthAddress, EthAddress, U256)> {
    let topics = log.topics();
    if topics.first() != Some(&TRANSFER_TOPIC) || topics.len() != 3 || log.data().len() != 32 {
        return None;
    }
    Some((
        EthAddress::from_word(topics[1]),
        EthAddress::from_word(topics[2]),
        U256::from_be_slice(log.data()),
    ))
}

fn wrapping_of(log: &EthLog, topic: B256) -> Option<(EthAddress, U256)> {
    let topics = log.topics();
    if topics.first() != Some(&topic) || topics.len() != 2 || log.data().len() != 32 {
        return None;
    }
    Some((
        EthAddress::from_word(topics[1]),
        U256::from_be_slice(log.data()),
    ))
}

pub fn wrapped_native(chain_id: u64) -> Option<EthAddress> {
    let raw = match chain_id {
        1 => "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
        10 | 8453 => "0x4200000000000000000000000000000000000006",
        _ => return None,
    };
    raw.parse().ok()
}

#[derive(Clone, Copy, Debug)]
pub struct TaintPolicy {
    pub min_share_bps: u64,
}

impl Default for TaintPolicy {
    fn default() -> Self {
        Self { min_share_bps: 100 }
    }
}

#[derive(Debug, Default)]
pub struct TaintScope {
    pub seeds: HashSet<EthAddress>,
    pub tracked: HashSet<EthAddress>,
    pub sinks: HashSet<EthAddress>,
}

impl TaintScope {
    fn holds(&self, address: &EthAddress) -> bool {
        (self.tracked.contains(address) || self.seeds.contains(address))
            && !self.sinks.contains(address)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Holding {
    total: U256,
    tainted: U256,
}

impl Holding {
    pub fn total(&self) -> U256 {
        self.total
    }

    pub fn tainted(&self) -> U256 {
        self.tainted
    }

    fn credit(&mut self, amount: U256, tainted: U256) {
        self.total = self.total.saturating_add(amount);
        self.tainted = self.tainted.saturating_add(tainted.min(amount));
    }

    fn debit(&mut self, amount: U256, unseen_tainted: bool) -> U256 {
        if amount > self.total {
            let unseen = amount - self.total;
            self.total = amount;
            if unseen_tainted {
                self.tainted = self.tainted.saturating_add(unseen);
            }
        }
        let tainted = share(amount, self.tainted, self.total);
        self.tainted = self.tainted.saturating_sub(tainted);
        self.total = self.total.saturating_sub(amount);
        tainted
    }
}

pub fn share(amount: U256, part: U256, whole: U256) -> U256 {
    if whole.is_zero() || part.is_zero() {
        return U256::ZERO;
    }
    if part >= whole {
        return amount;
    }
    let product: U512 = amount.widening_mul(part);
    let quotient = product / U512::from(whole);
    let limbs = quotient.as_limbs();
    if limbs[4..].iter().any(|limb| *limb != 0) {
        return amount;
    }
    U256::from_limbs([limbs[0], limbs[1], limbs[2], limbs[3]])
}

#[derive(Clone, Debug)]
pub struct TaintedMove {
    meta: TxMeta,
    slot: u32,
    kind: MoveKind,
    asset: Asset,
    from: EthAddress,
    to: EthAddress,
    amount: U256,
    tainted: U256,
}

impl TaintedMove {
    pub fn meta(&self) -> &TxMeta {
        &self.meta
    }

    pub fn tx_hash(&self) -> &TxHash {
        self.meta.tx_hash()
    }

    pub fn slot(&self) -> u32 {
        self.slot
    }

    pub fn kind(&self) -> MoveKind {
        self.kind
    }

    pub fn asset(&self) -> &Asset {
        &self.asset
    }

    pub fn from(&self) -> &EthAddress {
        &self.from
    }

    pub fn to(&self) -> &EthAddress {
        &self.to
    }

    pub fn amount(&self) -> U256 {
        self.amount
    }

    pub fn tainted(&self) -> U256 {
        self.tainted
    }

    pub fn share_bps(&self) -> u64 {
        let bps = share(U256::from(FULL_SHARE_BPS), self.tainted, self.amount);
        bps.try_into().unwrap_or(FULL_SHARE_BPS)
    }

    pub fn source(&self) -> Option<TagSource> {
        match self.kind {
            MoveKind::Payout => Some(TagSource::heuristic("eth_payout_from_logs", 1)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SwapThrough {
    pub meta: TxMeta,
    pub slot: u32,
    pub pool: EthAddress,
}

#[derive(Debug, Default)]
pub struct TaintTrace {
    moves: Vec<TaintedMove>,
    swaps: Vec<SwapThrough>,
    held: Vec<(EthAddress, Asset, Holding)>,
    transit: HashSet<EthAddress>,
    stranded: Vec<EthAddress>,
}

impl TaintTrace {
    pub fn moves(&self) -> &[TaintedMove] {
        &self.moves
    }

    pub fn swaps(&self) -> &[SwapThrough] {
        &self.swaps
    }

    pub fn held(&self) -> &[(EthAddress, Asset, Holding)] {
        &self.held
    }

    pub fn transit(&self) -> &HashSet<EthAddress> {
        &self.transit
    }

    pub fn reached<'a>(&'a self, scope: &'a TaintScope) -> impl Iterator<Item = EthAddress> + 'a {
        self.stranded.iter().copied().filter(move |to| {
            !scope.tracked.contains(to)
                && !scope.seeds.contains(to)
                && !scope.sinks.contains(to)
                && !self.transit.contains(to)
                && *to != EthAddress::ZERO
        })
    }
}

type Key = (EthAddress, Asset);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Inflow {
    total: U256,
    tainted: U256,
}

pub fn order(flows: &mut Vec<TxFlow>) {
    let transit: HashSet<EthAddress> = flows.iter().flat_map(TxFlow::pools).copied().collect();
    flows.sort_by(|left, right| {
        left.meta
            .block_number()
            .cmp(&right.meta.block_number())
            .then_with(|| left.meta.tx_hash().cmp(right.meta.tx_hash()))
    });

    let mut ordered = Vec::with_capacity(flows.len());
    let mut rest = std::mem::take(flows).into_iter().peekable();
    while let Some(first) = rest.next() {
        let block = first.meta.block_number();
        let mut same = vec![first];
        while let Some(next) = rest.next_if(|flow| flow.meta.block_number() == block) {
            same.push(next);
        }
        ordered.extend(order_block(same, &transit));
    }
    *flows = ordered;
}

fn order_block(flows: Vec<TxFlow>, transit: &HashSet<EthAddress>) -> Vec<TxFlow> {
    if flows.len() < 2 {
        return flows;
    }

    let holders = |pick: fn(&Move) -> EthAddress| -> Vec<HashSet<EthAddress>> {
        flows
            .iter()
            .map(|flow| {
                flow.moves
                    .iter()
                    .map(pick)
                    .filter(|address| !transit.contains(address))
                    .collect()
            })
            .collect()
    };
    let credits = holders(|moved| moved.to);
    let debits = holders(|moved| moved.from);

    let mut waits: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); flows.len()];
    for (later, debited) in debits.iter().enumerate() {
        for (earlier, credited) in credits.iter().enumerate() {
            if earlier != later && !debited.is_disjoint(credited) {
                waits[later].insert(earlier);
            }
        }
    }

    let mut done = vec![false; flows.len()];
    let mut picked = Vec::with_capacity(flows.len());
    while picked.len() < flows.len() {
        let next = (0..flows.len())
            .find(|index| !done[*index] && waits[*index].iter().all(|wait| done[*wait]))
            .or_else(|| (0..flows.len()).find(|index| !done[*index]));
        let Some(next) = next else {
            break;
        };
        done[next] = true;
        picked.push(next);
    }

    let mut slots: Vec<Option<TxFlow>> = flows.into_iter().map(Some).collect();
    picked
        .into_iter()
        .filter_map(|index| slots[index].take())
        .collect()
}

pub fn trace(flows: &[TxFlow], scope: &TaintScope, policy: TaintPolicy) -> TaintTrace {
    let transit: HashSet<EthAddress> = flows.iter().flat_map(TxFlow::pools).copied().collect();
    let mut held: HashMap<Key, Holding> = HashMap::new();
    let mut moves = Vec::new();
    let mut swaps = Vec::new();
    let mut stranded: HashSet<EthAddress> = HashSet::new();

    for flow in flows {
        let settled = settle(flow, scope, &transit, &held);
        for (key, holding) in settled.held {
            held.insert(key, holding);
        }
        for (key, left) in &settled.left {
            let received = settled.received.get(key).copied().unwrap_or_default();
            if significant(received, left.tainted, policy) {
                stranded.insert(key.0);
            }
        }

        for (moved, tainted) in flow.moves.iter().zip(settled.tainted) {
            if !significant(moved.amount, tainted, policy) {
                continue;
            }
            moves.push(TaintedMove {
                meta: flow.meta.clone(),
                slot: moved.slot,
                kind: moved.kind,
                asset: moved.asset.clone(),
                from: moved.from,
                to: moved.to,
                amount: moved.amount,
                tainted,
            });
        }

        for conversion in &flow.conversions {
            let fed = settled
                .inflow
                .get(&(conversion.pool, conversion.asset_in.clone()))
                .is_some_and(|inflow| significant(inflow.total, inflow.tainted, policy));
            if fed && conversion.slot < SYNTHETIC_SLOT {
                swaps.push(SwapThrough {
                    meta: flow.meta.clone(),
                    slot: conversion.slot,
                    pool: conversion.pool,
                });
            }
        }
    }

    let mut held: Vec<(EthAddress, Asset, Holding)> = held
        .into_iter()
        .filter(|(_, holding)| !holding.tainted.is_zero())
        .map(|((address, asset), holding)| (address, asset, holding))
        .collect();
    held.sort_by_key(|(address, _, _)| address.hex());
    let mut stranded: Vec<EthAddress> = stranded.into_iter().collect();
    stranded.sort_by_key(EthAddress::hex);

    TaintTrace {
        moves,
        swaps,
        held,
        transit,
        stranded,
    }
}

fn significant(amount: U256, tainted: U256, policy: TaintPolicy) -> bool {
    !tainted.is_zero()
        && tainted.saturating_mul(U256::from(FULL_SHARE_BPS))
            >= amount.saturating_mul(U256::from(policy.min_share_bps))
}

struct Settled {
    tainted: Vec<U256>,
    held: HashMap<Key, Holding>,
    inflow: HashMap<Key, Inflow>,
    left: HashMap<Key, Holding>,
    received: HashMap<Key, U256>,
}

fn settle(
    flow: &TxFlow,
    scope: &TaintScope,
    transit: &HashSet<EthAddress>,
    held: &HashMap<Key, Holding>,
) -> Settled {
    let mut inflow: HashMap<Key, Inflow> = HashMap::new();
    let mut settled = pass(flow, scope, transit, held, &inflow);

    for _ in 0..SETTLE_PASSES {
        if settled.inflow == inflow {
            break;
        }
        inflow = std::mem::take(&mut settled.inflow);
        settled = pass(flow, scope, transit, held, &inflow);
    }

    settled
}

fn pass(
    flow: &TxFlow,
    scope: &TaintScope,
    transit: &HashSet<EthAddress>,
    held: &HashMap<Key, Holding>,
    fed: &HashMap<Key, Inflow>,
) -> Settled {
    let mut lasting: HashMap<Key, Holding> = HashMap::new();
    let mut fleeting: HashMap<Key, Holding> = HashMap::new();
    let mut inflow: HashMap<Key, Inflow> = HashMap::new();
    let mut received: HashMap<Key, U256> = HashMap::new();
    let mut tainted = Vec::with_capacity(flow.moves.len());

    for moved in &flow.moves {
        let from_key = (moved.from, moved.asset.clone());
        let taken = if transit.contains(&moved.from) {
            share(
                moved.amount,
                passed_through(flow, fed, &moved.from, &moved.asset, Part::Tainted),
                passed_through(flow, fed, &moved.from, &moved.asset, Part::Total),
            )
        } else if scope.holds(&moved.from) {
            let unseen_tainted = scope.seeds.contains(&moved.from) && moved.from == flow.sender;
            lasting
                .entry(from_key)
                .or_insert_with_key(|key| held.get(key).copied().unwrap_or_default())
                .debit(moved.amount, unseen_tainted)
        } else {
            fleeting
                .entry(from_key)
                .or_default()
                .debit(moved.amount, false)
        };

        let to_key = (moved.to, moved.asset.clone());
        let given = if scope.seeds.contains(&moved.to) {
            moved.amount
        } else {
            taken
        };
        if transit.contains(&moved.to) {
            let entry = inflow.entry(to_key).or_default();
            entry.total = entry.total.saturating_add(moved.amount);
            entry.tainted = entry.tainted.saturating_add(given);
        } else if scope.holds(&moved.to) {
            lasting
                .entry(to_key)
                .or_insert_with_key(|key| held.get(key).copied().unwrap_or_default())
                .credit(moved.amount, given);
        } else {
            let got = received.entry(to_key.clone()).or_default();
            *got = got.saturating_add(given);
            fleeting
                .entry(to_key)
                .or_default()
                .credit(moved.amount, given);
        }

        tainted.push(taken);
    }

    Settled {
        tainted,
        held: lasting,
        inflow,
        left: fleeting,
        received,
    }
}

#[derive(Clone, Copy)]
enum Part {
    Total,
    Tainted,
}

fn passed_through(
    flow: &TxFlow,
    fed: &HashMap<Key, Inflow>,
    pool: &EthAddress,
    asset: &Asset,
    part: Part,
) -> U256 {
    let pick = |inflow: &Inflow| match part {
        Part::Total => inflow.total,
        Part::Tainted => inflow.tainted,
    };

    let converted: Vec<&Conversion> = flow
        .conversions
        .iter()
        .filter(|conversion| conversion.pool == *pool && conversion.asset_out == *asset)
        .collect();

    if converted.is_empty() {
        return fed
            .get(&(*pool, asset.clone()))
            .map(pick)
            .unwrap_or_default();
    }

    converted
        .into_iter()
        .map(|conversion| {
            let Some(inflow) = fed.get(&(*pool, conversion.asset_in.clone())) else {
                return U256::ZERO;
            };
            match part {
                Part::Total => conversion.amount_out,
                Part::Tainted => share(conversion.amount_out, inflow.tainted, inflow.total),
            }
        })
        .fold(U256::ZERO, U256::saturating_add)
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Bytes, TxHash};
    use domain::eth::{
        EthReceipt, EthTx, PoolInfo, PoolKey, Swap, SwapFamily, TagSource, graph::InteractionKind,
    };

    use super::*;

    const SEED: u8 = 0x51;
    const WETH: u8 = 0xee;
    const USDC: u8 = 0xc0;

    fn address(last_byte: u8) -> EthAddress {
        EthAddress::from([last_byte; 20])
    }

    fn usdc() -> Asset {
        Asset::Token(address(USDC))
    }

    fn weth() -> Asset {
        Asset::Token(address(WETH))
    }

    fn tx(block: u64, hash: u8, sender: u8, to: u8, value: u64, data: &'static [u8]) -> EthTx {
        EthTx::builder()
            .tx_hash(TxHash::with_last_byte(hash))
            .block_number(block)
            .timestamp(block * 12)
            .amount(U256::from(value))
            .from(address(sender))
            .to(address(to))
            .data(Bytes::from_static(data))
            .build()
    }

    fn moved(asset: Asset, from: u8, to: u8, amount: u64) -> Move {
        Move {
            slot: 0,
            kind: MoveKind::Transfer,
            asset,
            from: address(from),
            to: address(to),
            amount: U256::from(amount),
        }
    }

    fn flow(block: u64, hash: u8, sender: u8, moves: Vec<Move>) -> TxFlow {
        TxFlow {
            meta: TxMeta::from(&tx(block, hash, sender, 0x99, 0, &[])),
            sender: address(sender),
            moves,
            conversions: Vec::new(),
        }
    }

    fn swapping(mut flow: TxFlow, pool: u8, asset_in: Asset, asset_out: Asset, out: u64) -> TxFlow {
        flow.conversions.push(Conversion {
            slot: 7,
            pool: address(pool),
            asset_in,
            asset_out,
            amount_out: U256::from(out),
        });
        flow
    }

    fn scope(tracked: &[u8]) -> TaintScope {
        TaintScope {
            seeds: HashSet::from([address(SEED)]),
            tracked: tracked.iter().copied().map(address).collect(),
            sinks: HashSet::new(),
        }
    }

    fn everything() -> TaintPolicy {
        TaintPolicy { min_share_bps: 0 }
    }

    fn tainted_to(trace: &TaintTrace, to: u8) -> U256 {
        trace
            .moves()
            .iter()
            .filter(|moved| *moved.to() == address(to))
            .map(TaintedMove::tainted)
            .fold(U256::ZERO, U256::saturating_add)
    }

    #[test]
    fn share_takes_the_proportional_part() {
        assert_eq!(
            share(U256::from(100), U256::from(1), U256::from(4)),
            U256::from(25)
        );
        assert_eq!(
            share(U256::from(100), U256::from(9), U256::from(4)),
            U256::from(100)
        );
        assert_eq!(share(U256::from(100), U256::ZERO, U256::ZERO), U256::ZERO);
        assert_eq!(
            share(U256::MAX, U256::MAX - U256::from(1), U256::MAX),
            U256::MAX - U256::from(1)
        );
    }

    #[test]
    fn a_mixed_balance_passes_on_its_tainted_share() {
        let flows = vec![
            flow(1, 1, 0x0c, vec![moved(usdc(), 0x0c, 0x0a, 100)]),
            flow(2, 2, SEED, vec![moved(usdc(), SEED, 0x0a, 100)]),
            flow(3, 3, 0x0a, vec![moved(usdc(), 0x0a, 0x0b, 100)]),
        ];

        let trace = trace(&flows, &scope(&[SEED, 0x0a]), everything());

        assert_eq!(tainted_to(&trace, 0x0b), U256::from(50));
        assert_eq!(trace.held().len(), 1);
        assert_eq!(trace.held()[0].2.tainted(), U256::from(50));
    }

    #[test]
    fn money_sent_before_the_theft_stays_clean() {
        let flows = vec![
            flow(1, 1, 0x0a, vec![moved(usdc(), 0x0a, 0x0b, 100)]),
            flow(2, 2, SEED, vec![moved(usdc(), SEED, 0x0a, 100)]),
        ];

        let trace = trace(&flows, &scope(&[SEED, 0x0a]), everything());

        assert_eq!(tainted_to(&trace, 0x0b), U256::ZERO);
        assert_eq!(tainted_to(&trace, 0x0a), U256::from(100));
    }

    #[test]
    fn an_untracked_holder_passes_taint_only_within_the_same_tx() {
        let flows = vec![
            flow(1, 1, SEED, vec![moved(usdc(), SEED, 0x0a, 100)]),
            flow(2, 2, 0x0a, vec![moved(usdc(), 0x0a, 0x0b, 100)]),
            flow(
                3,
                3,
                SEED,
                vec![moved(usdc(), SEED, 0x0c, 10), moved(usdc(), 0x0c, 0x0d, 10)],
            ),
        ];

        let trace = trace(&flows, &scope(&[SEED]), everything());

        assert_eq!(tainted_to(&trace, 0x0b), U256::ZERO);
        assert_eq!(tainted_to(&trace, 0x0d), U256::from(10));
        let reached: HashSet<EthAddress> = trace.reached(&scope(&[SEED])).collect();
        assert_eq!(reached, HashSet::from([address(0x0a), address(0x0d)]));
    }

    #[test]
    fn a_swap_converts_taint_even_when_the_pool_pays_first() {
        let swap = swapping(
            flow(
                2,
                2,
                0x0a,
                vec![
                    moved(weth(), 0x70, 0x0a, 1),
                    moved(usdc(), 0x0a, 0x70, 3000),
                ],
            ),
            0x70,
            usdc(),
            weth(),
            1,
        );
        let flows = vec![
            flow(1, 1, SEED, vec![moved(usdc(), SEED, 0x0a, 3000)]),
            swap,
            flow(3, 3, 0x0a, vec![moved(weth(), 0x0a, 0x0b, 1)]),
        ];

        let trace = trace(&flows, &scope(&[SEED, 0x0a]), everything());

        assert_eq!(tainted_to(&trace, 0x0b), U256::from(1));
        assert!(trace.transit().contains(&address(0x70)));
        assert_eq!(trace.swaps().len(), 1);
    }

    #[test]
    fn a_half_tainted_swap_pays_out_half_tainted() {
        let swap = swapping(
            flow(
                2,
                2,
                0x0a,
                vec![
                    moved(usdc(), 0x0a, 0x70, 4000),
                    moved(weth(), 0x70, 0x0a, 2),
                ],
            ),
            0x70,
            usdc(),
            weth(),
            2,
        );
        let flows = vec![
            flow(1, 1, SEED, vec![moved(usdc(), SEED, 0x0a, 2000)]),
            flow(1, 0, 0x0c, vec![moved(usdc(), 0x0c, 0x0a, 2000)]),
            swap,
        ];

        let trace = trace(&flows, &scope(&[SEED, 0x0a]), everything());

        let weth_back = trace
            .moves()
            .iter()
            .find(|moved| *moved.from() == address(0x70))
            .map(TaintedMove::tainted);
        assert_eq!(weth_back, Some(U256::from(1)));
    }

    #[test]
    fn a_pool_does_not_keep_taint_for_later_traders() {
        let flows = vec![
            flow(1, 1, SEED, vec![moved(usdc(), SEED, 0x70, 100)]),
            swapping(
                flow(2, 2, 0x0c, vec![moved(usdc(), 0x70, 0x0c, 100)]),
                0x70,
                weth(),
                usdc(),
                100,
            ),
        ];

        let trace = trace(&flows, &scope(&[SEED]), everything());

        assert_eq!(tainted_to(&trace, 0x0c), U256::ZERO);
    }

    #[test]
    fn a_forged_transfer_from_a_seed_carries_nothing() {
        let flows = vec![flow(
            1,
            1,
            0x66,
            vec![moved(Asset::Token(address(0x67)), SEED, 0x0b, 1_000_000)],
        )];

        let trace = trace(&flows, &scope(&[SEED]), everything());

        assert_eq!(tainted_to(&trace, 0x0b), U256::ZERO);
    }

    #[test]
    fn a_seed_spends_its_own_balance_as_tainted() {
        let flows = vec![flow(1, 1, SEED, vec![moved(usdc(), SEED, 0x0b, 500)])];

        let trace = trace(&flows, &scope(&[SEED]), everything());

        assert_eq!(tainted_to(&trace, 0x0b), U256::from(500));
    }

    #[test]
    fn a_sink_does_not_pass_taint_on_later() {
        let mut scope = scope(&[SEED, 0x0a]);
        scope.sinks.insert(address(0x0a));
        let flows = vec![
            flow(1, 1, SEED, vec![moved(usdc(), SEED, 0x0a, 100)]),
            flow(2, 2, 0x0a, vec![moved(usdc(), 0x0a, 0x0b, 100)]),
        ];

        let trace = trace(&flows, &scope, everything());

        assert_eq!(tainted_to(&trace, 0x0b), U256::ZERO);
        assert_eq!(trace.reached(&scope).count(), 0);
    }

    #[test]
    fn a_small_share_is_dropped_by_the_threshold() {
        let flows = vec![
            flow(1, 1, 0x0c, vec![moved(usdc(), 0x0c, 0x0a, 990)]),
            flow(2, 2, SEED, vec![moved(usdc(), SEED, 0x0a, 10)]),
            flow(3, 3, 0x0a, vec![moved(usdc(), 0x0a, 0x0b, 1000)]),
        ];

        let trace = trace(
            &flows,
            &scope(&[SEED, 0x0a]),
            TaintPolicy { min_share_bps: 500 },
        );

        assert_eq!(tainted_to(&trace, 0x0b), U256::ZERO);
        assert_eq!(tainted_to(&trace, 0x0a), U256::from(10));
    }

    #[test]
    fn within_a_block_the_receipt_goes_before_the_spend() {
        let mut flows = vec![
            flow(5, 1, 0x0a, vec![moved(usdc(), 0x0a, 0x0b, 100)]),
            flow(5, 2, SEED, vec![moved(usdc(), SEED, 0x0a, 100)]),
            flow(4, 3, 0x0c, vec![moved(usdc(), 0x0c, 0x0d, 1)]),
        ];

        order(&mut flows);

        let hashes: Vec<u8> = flows
            .iter()
            .map(|flow| flow.meta().tx_hash().0[31])
            .collect();
        assert_eq!(hashes, vec![3, 2, 1]);
        let trace = trace(&flows, &scope(&[SEED, 0x0a]), everything());
        assert_eq!(tainted_to(&trace, 0x0b), U256::from(100));
    }

    fn log(address: EthAddress, topics: Vec<B256>, amount: u64) -> EthLog {
        EthLog::new(
            address,
            topics,
            Bytes::from(U256::from(amount).to_be_bytes::<32>().to_vec()),
        )
    }

    fn word(byte: u8) -> B256 {
        address(byte).address().into_word()
    }

    #[test]
    fn a_router_unwrap_pays_the_sender_in_native() {
        let router = 0x40;
        let pair = 0x70;
        let receipt = EthReceipt::new(
            true,
            None,
            vec![
                log(
                    address(USDC),
                    vec![TRANSFER_TOPIC, word(0x0a), word(pair)],
                    3000,
                ),
                log(
                    address(WETH),
                    vec![TRANSFER_TOPIC, word(pair), word(router)],
                    1,
                ),
                log(address(WETH), vec![WITHDRAWAL_TOPIC, word(router)], 1),
            ],
        );
        let pool = PoolInfo::new(
            PoolKey::new(address(pair), None),
            SwapFamily::UniswapV2,
            "uniswap_v2".to_owned(),
            vec![address(USDC), address(WETH)],
            Some(3000),
            None,
            TagSource::heuristic("test", 1),
        );
        let swap = Interaction::new(
            receipt.clone(),
            InteractionKind::ContractInteraction {
                contract_address: address(router),
                interactor: address(0x0a),
                contract_interaction_type: ContractAction::Swap(Swap::new(
                    pool,
                    address(USDC),
                    U256::from(3000),
                    address(WETH),
                    U256::from(1),
                )),
            },
        );
        let interactions = vec![
            Interaction::new(receipt.clone(), InteractionKind::Protocol),
            Interaction::new(receipt.clone(), InteractionKind::Protocol),
            swap,
        ];
        let mined = MinedTx::new(
            tx(2, 2, 0x0a, router, 0, &[0x18, 0xcb, 0xaf, 0xe5]),
            receipt,
        );
        let swap_flow = TxFlow::read(&mined, &interactions, Some(&address(WETH)))
            .expect("a successful tx reads into a flow");
        let flows = vec![
            flow(1, 1, SEED, vec![moved(usdc(), SEED, 0x0a, 3000)]),
            swap_flow,
            flow(
                3,
                3,
                0x0a,
                vec![Move {
                    kind: MoveKind::Value,
                    ..moved(Asset::Native, 0x0a, 0x0b, 1)
                }],
            ),
        ];

        let trace = trace(&flows, &scope(&[SEED, 0x0a]), everything());

        let payout = trace
            .moves()
            .iter()
            .find(|moved| moved.kind() == MoveKind::Payout)
            .expect("the router pays the sender");
        assert_eq!(*payout.to(), address(0x0a));
        assert_eq!(payout.tainted(), U256::from(1));
        assert!(payout.source().is_some());
        assert_eq!(tainted_to(&trace, 0x0b), U256::from(1));
        assert_eq!(trace.swaps().len(), 1);
        assert_eq!(trace.swaps()[0].slot, 2);
    }

    #[test]
    fn mainnet_knows_its_wrapped_ether() {
        assert!(wrapped_native(1).is_some());
        assert!(wrapped_native(31337).is_none());
    }

    #[test]
    fn a_failed_tx_moves_nothing() {
        let mined = MinedTx::new(
            tx(1, 1, SEED, 0x0b, 5, &[]),
            EthReceipt::new(false, None, Vec::new()),
        );

        assert!(TxFlow::read(&mined, &[], None).is_none());
    }

    #[test]
    fn a_plain_value_transfer_keeps_the_first_slot() {
        let mined = MinedTx::new(
            tx(1, 1, SEED, 0x0b, 5, &[]),
            EthReceipt::new(true, None, Vec::new()),
        );

        let flow = TxFlow::read(&mined, &[], None).expect("a plain transfer reads");

        assert_eq!(flow.moves.len(), 1);
        assert_eq!(flow.moves[0].slot, 0);
        assert_eq!(flow.moves[0].kind, MoveKind::Value);
    }
}
