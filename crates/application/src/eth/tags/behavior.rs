use std::collections::HashSet;

use alloy_primitives::TxHash;

use domain::eth::{EthAddress, Tag, TagSource, TagSubject};

use super::{Flows, Heuristic, Transfer, rising};

const EVIDENCE: usize = 32;

pub struct FanPattern {
    pub floor: usize,
}

impl Default for FanPattern {
    fn default() -> Self {
        Self { floor: 10 }
    }
}

impl FanPattern {
    fn spread<'t, 'a: 't>(
        transfers: impl Iterator<Item = &'t Transfer<'a>>,
        peer: impl Fn(&Transfer<'a>) -> EthAddress,
    ) -> (usize, Vec<TxHash>) {
        let mut peers = HashSet::new();
        let mut txs = Vec::new();
        for transfer in transfers {
            peers.insert(peer(transfer));
            if txs.len() < EVIDENCE {
                txs.push(transfer.tx());
            }
        }
        (peers.len(), txs)
    }
}

impl Heuristic for FanPattern {
    fn source(&self) -> TagSource {
        TagSource::heuristic("fan_pattern", 1)
    }

    fn tag(&self, flows: &Flows<'_>) -> Vec<Tag> {
        let mut tags = Vec::new();

        for sender in flows.senders() {
            if flows.is_contract(sender) {
                continue;
            }
            let (peers, txs) = Self::spread(flows.sent_by(sender), |transfer| transfer.to);
            if peers >= self.floor {
                tags.push(
                    Tag::new(
                        TagSubject::Address(*sender),
                        "behavior.fan_out",
                        self.source(),
                    )
                    .with_confidence(rising(peers, self.floor, 0.5))
                    .with_evidence(txs)
                    .with_note(format!("sent to {peers} distinct addresses")),
                );
            }
        }

        for receiver in flows.receivers() {
            if flows.is_contract(receiver) {
                continue;
            }
            let (peers, txs) = Self::spread(flows.received_by(receiver), |transfer| transfer.from);
            if peers >= self.floor {
                tags.push(
                    Tag::new(
                        TagSubject::Address(*receiver),
                        "behavior.fan_in",
                        self.source(),
                    )
                    .with_confidence(rising(peers, self.floor, 0.5))
                    .with_evidence(txs)
                    .with_note(format!("received from {peers} distinct addresses")),
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
    fn many_receivers_make_a_fan_out() {
        let edges: Vec<_> = (0..12)
            .map(|peer| native(u64::from(peer), 10, 1, 50 + peer, 1))
            .collect();
        let tags = FanPattern::default().tag(&Flows::new(&edges, &[]));

        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].key(), "behavior.fan_out");
        assert_eq!(tags[0].subject(), &TagSubject::Address(address(1)));
    }

    #[test]
    fn a_few_receivers_are_ordinary() {
        let edges: Vec<_> = (0..3)
            .map(|peer| native(u64::from(peer), 10, 1, 50 + peer, 1))
            .collect();

        assert!(
            FanPattern::default()
                .tag(&Flows::new(&edges, &[]))
                .is_empty()
        );
    }
}
