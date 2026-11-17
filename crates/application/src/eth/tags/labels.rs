use domain::eth::{Tag, TagSource, TagSubject};

use super::{Flows, Heuristic};

const KEYWORDS: &[(&str, &[&str])] = &[
    (
        "entity.exchange",
        &[
            "binance",
            "coinbase",
            "kraken",
            "okx",
            "okex",
            "bybit",
            "kucoin",
            "huobi",
            "htx",
            "bitfinex",
            "gate.io",
            "bitget",
            "mexc",
            "crypto.com",
            "exchange",
        ],
    ),
    (
        "entity.bridge",
        &[
            "bridge",
            "wormhole",
            "stargate",
            "layerzero",
            "hop protocol",
            "across",
            "synapse",
            "multichain",
            "orbiter",
            "portal",
        ],
    ),
    (
        "entity.mixer",
        &["tornado", "mixer", "railgun", "sinbad", "blender"],
    ),
    (
        "entity.dex",
        &[
            "uniswap",
            "sushiswap",
            "curve",
            "1inch",
            "0x protocol",
            "pancakeswap",
            "router",
        ],
    ),
    (
        "entity.lending",
        &["aave", "compound", "morpho", "maker", "spark"],
    ),
    (
        "entity.sanctioned",
        &["ofac", "sanction", "exploit", "hacker", "phish", "scam"],
    ),
];

pub struct LabelKeywords;

impl Heuristic for LabelKeywords {
    fn source(&self) -> TagSource {
        TagSource::heuristic("label_keywords", 1)
    }

    fn tag(&self, flows: &Flows<'_>) -> Vec<Tag> {
        let mut tags = Vec::new();

        for actor in flows.actors() {
            for (key, words) in KEYWORDS {
                let hit = actor.labels().iter().find_map(|label| {
                    let value = label.value().to_lowercase();
                    words
                        .iter()
                        .find(|word| value.contains(*word))
                        .map(|word| (label, *word))
                });
                let Some((label, word)) = hit else {
                    continue;
                };
                tags.push(
                    Tag::new(TagSubject::Address(*actor.address()), *key, self.source())
                        .with_confidence(0.7)
                        .with_note(format!(
                            "label \"{}\" from {} mentions \"{word}\"",
                            label.value(),
                            label.source()
                        )),
                );
            }
        }

        tags
    }
}

#[cfg(test)]
mod tests {
    use domain::eth::{Actor, ActorKind, AddressLabel};

    use super::*;
    use crate::eth::tags::fixtures::address;

    #[test]
    fn a_provider_label_names_the_entity_and_keeps_its_origin() {
        let actors = vec![Actor::new(
            address(1),
            ActorKind::Eoa,
            vec![AddressLabel::new(
                "Binance 14".to_owned(),
                "etherscan-json".to_owned(),
            )],
        )];
        let tags = LabelKeywords.tag(&Flows::new(&[], &actors));

        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].key(), "entity.exchange");
        assert!(tags[0].note().contains("etherscan-json"));
    }
}
