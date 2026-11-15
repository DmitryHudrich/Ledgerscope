use std::collections::{HashMap, HashSet};

use alloy_primitives::{TxHash, U256};

use domain::eth::{EthAddress, InteractionKind, Tag, TagSource, TagSubject};

use super::{Flows, Heuristic, Moved, rising};

const DELEGATE: &str = "DelegateResource";
const UNDELEGATE: &str = "UnDelegateResource";

pub struct EnergyRental {
    pub lender_floor: usize,
}

impl Default for EnergyRental {
    fn default() -> Self {
        Self { lender_floor: 3 }
    }
}

impl Heuristic for EnergyRental {
    fn source(&self) -> TagSource {
        TagSource::heuristic("energy_rental", 1)
    }

    fn tag(&self, flows: &Flows<'_>) -> Vec<Tag> {
        let mut tags = Vec::new();
        let mut lent: HashMap<EthAddress, (HashSet<EthAddress>, Vec<TxHash>)> = HashMap::new();

        for edge in flows.edges() {
            let InteractionKind::SystemAction {
                action,
                from,
                to: Some(to),
                ..
            } = edge.interaction().kind()
            else {
                continue;
            };
            let key = match action.as_str() {
                DELEGATE => "gas.energy_rental",
                UNDELEGATE => "gas.energy_return",
                _ => continue,
            };
            let tx = *edge.meta().tx_hash();
            tags.push(
                Tag::new(TagSubject::Tx(tx), key, self.source())
                    .with_evidence(vec![tx])
                    .with_note(format!("{action} from {from} to {to}")),
            );
            if action == DELEGATE {
                let entry = lent.entry(*from).or_default();
                entry.0.insert(*to);
                entry.1.push(tx);
            }
        }

        for (lender, (borrowers, txs)) in lent {
            if borrowers.len() < self.lender_floor {
                continue;
            }
            tags.push(
                Tag::new(
                    TagSubject::Address(lender),
                    "gas.energy_lender",
                    self.source(),
                )
                .with_confidence(rising(borrowers.len(), self.lender_floor, 0.6))
                .with_evidence(txs)
                .with_note(format!(
                    "delegated resources to {} addresses",
                    borrowers.len()
                )),
            );
        }

        tags
    }
}

pub struct GasTopUp {
    pub max_wei: U256,
    pub window: u64,
    pub funder_floor: usize,
}

impl Default for GasTopUp {
    fn default() -> Self {
        Self {
            max_wei: U256::from(5_000_000_000_000_000_u64),
            window: 100,
            funder_floor: 3,
        }
    }
}

impl Heuristic for GasTopUp {
    fn source(&self) -> TagSource {
        TagSource::heuristic("gas_top_up", 1)
    }

    fn tag(&self, flows: &Flows<'_>) -> Vec<Tag> {
        let mut tags = Vec::new();
        let mut funded: HashMap<EthAddress, (HashSet<EthAddress>, Vec<TxHash>)> = HashMap::new();

        for transfer in flows.transfers() {
            let Moved::Native(wei) = transfer.moved else {
                continue;
            };
            if wei.is_zero() || wei > self.max_wei || flows.is_contract(&transfer.to) {
                continue;
            }
            let spent = flows.acted_by(&transfer.to).iter().find(|edge| {
                let block = edge.meta().block_number();
                block >= transfer.block()
                    && block <= transfer.block() + self.window
                    && *edge.meta().tx_hash() != transfer.tx()
                    && !matches!(
                        edge.interaction().kind(),
                        InteractionKind::NativeTransfer(_)
                    )
            });
            let Some(spent) = spent else {
                continue;
            };

            let tx = transfer.tx();
            tags.push(
                Tag::new(TagSubject::Tx(tx), "gas.top_up", self.source())
                    .with_confidence(0.6)
                    .with_evidence(vec![tx, *spent.meta().tx_hash()])
                    .with_note(format!(
                        "small native transfer, the receiver acts within {} blocks",
                        spent.meta().block_number() - transfer.block()
                    )),
            );
            let entry = funded.entry(transfer.from).or_default();
            entry.0.insert(transfer.to);
            entry.1.push(tx);
        }

        for (funder, (receivers, txs)) in funded {
            if receivers.len() < self.funder_floor {
                continue;
            }
            tags.push(
                Tag::new(TagSubject::Address(funder), "gas.funder", self.source())
                    .with_confidence(rising(receivers.len(), self.funder_floor, 0.55))
                    .with_evidence(txs)
                    .with_note(format!("topped up {} addresses with gas", receivers.len())),
            );
        }

        tags
    }
}

#[cfg(test)]
mod tests {
    use domain::eth::Tag;

    use super::*;
    use crate::eth::tags::fixtures::{action, address, native, token};

    fn keyed<'a>(tags: &'a [Tag], key: &str) -> Vec<&'a Tag> {
        tags.iter().filter(|tag| tag.key() == key).collect()
    }

    #[test]
    fn delegations_mark_the_rental_and_a_busy_lender() {
        let edges = vec![
            action(1, 10, DELEGATE, 1, 2),
            action(2, 11, DELEGATE, 1, 3),
            action(3, 12, DELEGATE, 1, 4),
            action(4, 13, UNDELEGATE, 1, 2),
            action(5, 14, "FreezeBalanceV2", 1, 1),
        ];
        let tags = EnergyRental::default().tag(&Flows::new(&edges, &[]));

        assert_eq!(keyed(&tags, "gas.energy_rental").len(), 3);
        assert_eq!(keyed(&tags, "gas.energy_return").len(), 1);
        let lender = keyed(&tags, "gas.energy_lender");
        assert_eq!(lender.len(), 1);
        assert_eq!(lender[0].subject(), &TagSubject::Address(address(1)));
    }

    #[test]
    fn a_small_transfer_followed_by_a_token_move_is_a_top_up() {
        let edges = vec![
            native(1, 10, 1, 2, 1_000_000_000_000_000),
            token(2, 12, 2, 9, 500),
            native(3, 10, 1, 3, 1_000_000_000_000_000),
            token(4, 20, 3, 9, 500),
            native(5, 10, 1, 4, 1_000_000_000_000_000),
            token(6, 30, 4, 9, 500),
        ];
        let tags = GasTopUp::default().tag(&Flows::new(&edges, &[]));

        assert_eq!(keyed(&tags, "gas.top_up").len(), 3);
        let funder = keyed(&tags, "gas.funder");
        assert_eq!(funder.len(), 1);
        assert_eq!(funder[0].subject(), &TagSubject::Address(address(1)));
        assert_eq!(funder[0].source().name(), "gas_top_up");
    }

    #[test]
    fn a_big_transfer_is_not_gas() {
        let edges = vec![
            native(1, 10, 1, 2, 2_000_000_000_000_000_000),
            token(2, 12, 2, 9, 500),
        ];

        assert!(GasTopUp::default().tag(&Flows::new(&edges, &[])).is_empty());
    }

    #[test]
    fn an_idle_receiver_is_not_a_top_up() {
        let edges = vec![
            native(1, 10, 1, 2, 1_000_000_000_000_000),
            token(2, 500, 2, 9, 500),
        ];

        assert!(GasTopUp::default().tag(&Flows::new(&edges, &[])).is_empty());
    }
}
