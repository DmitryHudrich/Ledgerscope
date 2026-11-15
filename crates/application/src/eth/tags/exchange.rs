use std::collections::{HashMap, HashSet};

use alloy_primitives::TxHash;

use domain::eth::{EthAddress, Tag, TagSource, TagSubject};

use super::{Flows, Heuristic, rising};

pub struct DepositSweep {
    pub target_floor: usize,
}

impl Default for DepositSweep {
    fn default() -> Self {
        Self { target_floor: 3 }
    }
}

struct Deposit {
    address: EthAddress,
    txs: Vec<TxHash>,
}

impl Heuristic for DepositSweep {
    fn source(&self) -> TagSource {
        TagSource::heuristic("deposit_sweep", 1)
    }

    fn tag(&self, flows: &Flows<'_>) -> Vec<Tag> {
        let mut by_target: HashMap<EthAddress, Vec<Deposit>> = HashMap::new();

        for candidate in flows.senders() {
            if flows.is_contract(candidate) {
                continue;
            }
            let targets: HashSet<EthAddress> = flows
                .sent_by(candidate)
                .map(|transfer| transfer.to)
                .collect();
            let [target] = targets.into_iter().collect::<Vec<_>>()[..] else {
                continue;
            };
            if target == *candidate {
                continue;
            }
            let funded: Vec<_> = flows
                .received_by(candidate)
                .filter(|transfer| transfer.from != target)
                .collect();
            let Some(first) = funded.first() else {
                continue;
            };
            let swept: Vec<TxHash> = flows
                .sent_by(candidate)
                .filter(|transfer| transfer.block() >= first.block())
                .map(|transfer| transfer.tx())
                .collect();
            if swept.is_empty() {
                continue;
            }
            let mut txs: Vec<TxHash> = funded.iter().map(|transfer| transfer.tx()).collect();
            txs.extend(swept);
            by_target.entry(target).or_default().push(Deposit {
                address: *candidate,
                txs,
            });
        }

        let mut tags = Vec::new();
        for (target, deposits) in by_target {
            if deposits.len() < self.target_floor {
                continue;
            }
            let confidence = rising(deposits.len(), self.target_floor, 0.5);
            tags.push(
                Tag::new(
                    TagSubject::Address(target),
                    "exchange.sweep_target",
                    self.source(),
                )
                .with_confidence(confidence)
                .with_evidence(
                    deposits
                        .iter()
                        .flat_map(|deposit| deposit.txs.last().copied())
                        .collect(),
                )
                .with_note(format!(
                    "{} addresses forward everything they receive here",
                    deposits.len()
                )),
            );
            for deposit in deposits {
                tags.push(
                    Tag::new(
                        TagSubject::Address(deposit.address),
                        "exchange.deposit_candidate",
                        self.source(),
                    )
                    .with_confidence(confidence * 0.9)
                    .with_evidence(deposit.txs)
                    .with_note(format!("forwards only to {target}")),
                );
            }
        }

        tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eth::tags::fixtures::{address, native};

    #[test]
    fn deposits_swept_into_one_wallet_are_marked() {
        let mut edges = Vec::new();
        for (seq, deposit) in [(10u64, 20u8), (20, 21), (30, 22)] {
            edges.push(native(seq, seq, 40 + deposit, deposit, 1_000));
            edges.push(native(seq + 1, seq + 1, deposit, 99, 990));
        }
        let tags = DepositSweep::default().tag(&Flows::new(&edges, &[]));

        let targets: Vec<_> = tags
            .iter()
            .filter(|tag| tag.key() == "exchange.sweep_target")
            .collect();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].subject(), &TagSubject::Address(address(99)));
        assert_eq!(
            tags.iter()
                .filter(|tag| tag.key() == "exchange.deposit_candidate")
                .count(),
            3
        );
    }

    #[test]
    fn an_address_paying_many_people_is_no_deposit() {
        let edges = vec![
            native(1, 1, 40, 20, 1_000),
            native(2, 2, 20, 99, 400),
            native(3, 3, 20, 98, 400),
        ];

        assert!(
            DepositSweep::default()
                .tag(&Flows::new(&edges, &[]))
                .is_empty()
        );
    }
}
