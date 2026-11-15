mod behavior;
mod exchange;
mod gas;
mod labels;

use std::collections::HashMap;

use alloy_primitives::{TxHash, U256};

use domain::eth::{
    Actor, ContractAction, EthAddress, InteractionEdge, InteractionKind, Tag, TagSource,
    graph::NativeTransfer,
};

pub use behavior::FanPattern;
pub use exchange::DepositSweep;
pub use gas::{EnergyRental, GasTopUp};
pub use labels::LabelKeywords;

pub trait Heuristic: Send + Sync {
    fn source(&self) -> TagSource;

    fn tag(&self, flows: &Flows<'_>) -> Vec<Tag>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Moved {
    Native(U256),
    Token(U256),
}

#[derive(Clone, Copy, Debug)]
pub struct Transfer<'a> {
    pub edge: &'a InteractionEdge,
    pub from: EthAddress,
    pub to: EthAddress,
    pub moved: Moved,
}

impl Transfer<'_> {
    pub fn tx(&self) -> TxHash {
        *self.edge.meta().tx_hash()
    }

    pub fn block(&self) -> u64 {
        self.edge.meta().block_number()
    }
}

fn transfer_of(edge: &InteractionEdge) -> Option<Transfer<'_>> {
    if !edge.interaction().receipt().succeeded() {
        return None;
    }
    let (from, to, moved) = match edge.interaction().kind() {
        InteractionKind::NativeTransfer(native) => native_parts(native),
        InteractionKind::AssetTransfer(asset) => {
            (*asset.from(), *asset.to(), Moved::Token(asset.amount()))
        }
        InteractionKind::ContractInteraction {
            contract_interaction_type:
                ContractAction::Erc20Transfer {
                    from, to, amount, ..
                },
            ..
        } => (*from, *to, Moved::Token(*amount)),
        _ => return None,
    };
    Some(Transfer {
        edge,
        from,
        to,
        moved,
    })
}

fn native_parts(native: &NativeTransfer) -> (EthAddress, EthAddress, Moved) {
    (*native.from(), *native.to(), Moved::Native(native.amount()))
}

pub struct Flows<'a> {
    edges: &'a [InteractionEdge],
    actors: HashMap<EthAddress, &'a Actor>,
    transfers: Vec<Transfer<'a>>,
    outgoing: HashMap<EthAddress, Vec<usize>>,
    incoming: HashMap<EthAddress, Vec<usize>>,
    acted: HashMap<EthAddress, Vec<&'a InteractionEdge>>,
}

impl<'a> Flows<'a> {
    pub fn new(edges: &'a [InteractionEdge], actors: &'a [Actor]) -> Self {
        let mut transfers: Vec<Transfer<'a>> = edges.iter().filter_map(transfer_of).collect();
        transfers.sort_by_key(|transfer| (transfer.block(), transfer.tx()));

        let mut outgoing: HashMap<EthAddress, Vec<usize>> = HashMap::new();
        let mut incoming: HashMap<EthAddress, Vec<usize>> = HashMap::new();
        for (index, transfer) in transfers.iter().enumerate() {
            outgoing.entry(transfer.from).or_default().push(index);
            incoming.entry(transfer.to).or_default().push(index);
        }

        let mut acted: HashMap<EthAddress, Vec<&'a InteractionEdge>> = HashMap::new();
        for edge in edges {
            if let Some((from, _)) = edge.endpoints() {
                acted.entry(*from).or_default().push(edge);
            }
        }
        for list in acted.values_mut() {
            list.sort_by_key(|edge| edge.meta().block_number());
        }

        Self {
            edges,
            actors: actors
                .iter()
                .map(|actor| (*actor.address(), actor))
                .collect(),
            transfers,
            outgoing,
            incoming,
            acted,
        }
    }

    pub fn edges(&self) -> &'a [InteractionEdge] {
        self.edges
    }

    pub fn actors(&self) -> impl Iterator<Item = &'a Actor> + '_ {
        self.actors.values().copied()
    }

    pub fn is_contract(&self, address: &EthAddress) -> bool {
        self.actors
            .get(address)
            .is_some_and(|actor| actor.is_contract())
    }

    pub fn transfers(&self) -> &[Transfer<'a>] {
        &self.transfers
    }

    pub fn sent_by(&self, address: &EthAddress) -> impl Iterator<Item = &Transfer<'a>> + '_ {
        self.outgoing
            .get(address)
            .into_iter()
            .flatten()
            .map(|index| &self.transfers[*index])
    }

    pub fn received_by(&self, address: &EthAddress) -> impl Iterator<Item = &Transfer<'a>> + '_ {
        self.incoming
            .get(address)
            .into_iter()
            .flatten()
            .map(|index| &self.transfers[*index])
    }

    pub fn senders(&self) -> impl Iterator<Item = &EthAddress> + '_ {
        self.outgoing.keys()
    }

    pub fn receivers(&self) -> impl Iterator<Item = &EthAddress> + '_ {
        self.incoming.keys()
    }

    pub fn acted_by(&self, address: &EthAddress) -> &[&'a InteractionEdge] {
        self.acted
            .get(address)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

pub struct TagEngine {
    heuristics: Vec<Box<dyn Heuristic>>,
}

impl TagEngine {
    pub fn new(heuristics: Vec<Box<dyn Heuristic>>) -> Self {
        Self { heuristics }
    }

    pub fn none() -> Self {
        Self::new(Vec::new())
    }

    pub fn standard() -> Self {
        Self::new(vec![
            Box::new(EnergyRental::default()),
            Box::new(GasTopUp::default()),
            Box::new(FanPattern::default()),
            Box::new(DepositSweep::default()),
            Box::new(LabelKeywords),
        ])
    }

    pub fn sources(&self) -> Vec<TagSource> {
        self.heuristics
            .iter()
            .map(|heuristic| heuristic.source())
            .collect()
    }

    pub fn tag(&self, edges: &[InteractionEdge], actors: &[Actor]) -> Vec<Tag> {
        if self.heuristics.is_empty() {
            return Vec::new();
        }
        let flows = Flows::new(edges, actors);
        self.heuristics
            .iter()
            .flat_map(|heuristic| heuristic.tag(&flows))
            .collect()
    }
}

impl Default for TagEngine {
    fn default() -> Self {
        Self::standard()
    }
}

pub(crate) fn rising(count: usize, floor: usize, base: f32) -> f32 {
    let extra = count.saturating_sub(floor) as f32;
    (base + extra * 0.05).min(0.95)
}

#[cfg(test)]
pub(crate) mod fixtures {
    use alloy_primitives::{TxHash, U256};

    use domain::eth::{
        ContractAction, EthAddress, EthReceipt, EthTx, Interaction, InteractionEdge,
        InteractionKind, TxMeta, graph::NativeTransfer,
    };

    pub fn address(byte: u8) -> EthAddress {
        EthAddress::from([byte; 20])
    }

    fn meta(seq: u64, block: u64) -> TxMeta {
        let tx = EthTx::builder()
            .tx_hash(TxHash::from(U256::from(seq)))
            .block_number(block)
            .timestamp(block * 12)
            .amount(U256::ZERO)
            .from(address(0))
            .to(address(0))
            .data(Default::default())
            .build();
        TxMeta::from(&tx)
    }

    pub fn native(seq: u64, block: u64, from: u8, to: u8, wei: u64) -> InteractionEdge {
        let tx = EthTx::builder()
            .tx_hash(TxHash::from(U256::from(seq)))
            .block_number(block)
            .timestamp(block * 12)
            .amount(U256::from(wei))
            .from(address(from))
            .to(address(to))
            .data(Default::default())
            .build();
        let meta = TxMeta::from(&tx);
        let transfer = NativeTransfer::try_from(tx).expect("a plain transfer");
        InteractionEdge::new(
            meta,
            0,
            Interaction::new(
                EthReceipt::new(true, None, Vec::new()),
                InteractionKind::NativeTransfer(transfer),
            ),
        )
    }

    pub fn token(seq: u64, block: u64, from: u8, to: u8, amount: u64) -> InteractionEdge {
        InteractionEdge::new(
            meta(seq, block),
            0,
            Interaction::new(
                EthReceipt::new(true, None, Vec::new()),
                InteractionKind::ContractInteraction {
                    contract_address: address(200),
                    interactor: address(from),
                    contract_interaction_type: ContractAction::Erc20Transfer {
                        token: address(200),
                        from: address(from),
                        to: address(to),
                        amount: U256::from(amount),
                    },
                },
            ),
        )
    }

    pub fn action(seq: u64, block: u64, name: &str, from: u8, to: u8) -> InteractionEdge {
        InteractionEdge::new(
            meta(seq, block),
            0,
            Interaction::new(
                EthReceipt::new(true, None, Vec::new()),
                InteractionKind::SystemAction {
                    action: name.to_owned(),
                    from: address(from),
                    to: Some(address(to)),
                    amount: U256::from(1_000_u64),
                },
            ),
        )
    }
}
