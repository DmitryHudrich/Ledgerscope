use std::collections::BTreeMap;

use adapters::eth::{
    Chainlist, ETHEREUM,
    networks::{self, ETHEREUM_TESTNETS, KNOWN_DEVNETS},
    served_chain, tron,
};
use reqwest::{Client, Url};

use crate::{
    config::{Config, FamilyConfig, NetworkKind},
    state::Family,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Mainnet,
    Testnet,
    Devnet,
}

impl Network {
    pub fn name(self) -> &'static str {
        match self {
            Self::Mainnet => "mainnet",
            Self::Testnet => "testnet",
            Self::Devnet => "devnet",
        }
    }
}

impl From<NetworkKind> for Network {
    fn from(kind: NetworkKind) -> Self {
        match kind {
            NetworkKind::Mainnet => Self::Mainnet,
            NetworkKind::Testnet => Self::Testnet,
            NetworkKind::Devnet => Self::Devnet,
        }
    }
}

impl From<FamilyConfig> for Family {
    fn from(family: FamilyConfig) -> Self {
        match family {
            FamilyConfig::Evm => Self::Evm,
            FamilyConfig::Tron => Self::Tron,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub family: Family,
    pub network: Network,
    pub own: Vec<Url>,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
struct Found {
    family: Family,
    network: Network,
    own: Vec<Url>,
    name: Option<String>,
}

impl Found {
    fn public(family: Family, network: Network) -> Self {
        Self {
            family,
            network,
            own: Vec::new(),
            name: None,
        }
    }
}

pub async fn plan(
    config: &Config,
    chainlist: Option<&Chainlist>,
    http: &Client,
) -> BTreeMap<u64, Plan> {
    let mut found = BTreeMap::new();
    mainnets(config, chainlist, &mut found);
    if config.chains.testnets {
        testnets(config, chainlist, &mut found);
    }
    if config.chains.devnets {
        devnets(config, chainlist, http, &mut found).await;
    }

    config
        .chain_ids(found.keys().copied())
        .into_iter()
        .map(|chain_id| {
            let found = found.get(&chain_id);
            let network = config.network(chain_id);
            let plan = Plan {
                family: network
                    .and_then(|network| network.family)
                    .map(Family::from)
                    .or(found.map(|found| found.family))
                    .unwrap_or_else(|| Family::of(chain_id)),
                network: network
                    .and_then(|network| network.kind)
                    .map(Network::from)
                    .or(found.map(|found| found.network))
                    .unwrap_or_else(|| kind_of(chainlist, chain_id)),
                own: found.map(|found| found.own.clone()).unwrap_or_default(),
                name: found.and_then(|found| found.name.clone()),
            };
            (chain_id, plan)
        })
        .collect()
}

fn kind_of(chainlist: Option<&Chainlist>, chain_id: u64) -> Network {
    if KNOWN_DEVNETS.contains(&chain_id) {
        Network::Devnet
    } else if ETHEREUM_TESTNETS.contains(&chain_id)
        || tron::TRON_TESTNETS.contains(&chain_id)
        || chainlist
            .and_then(|chainlist| chainlist.chain(chain_id))
            .is_some_and(|chain| chain.testnet())
    {
        Network::Testnet
    } else {
        Network::Mainnet
    }
}

fn mainnets(config: &Config, chainlist: Option<&Chainlist>, found: &mut BTreeMap<u64, Found>) {
    if config.chains.ethereum_l2 {
        match chainlist {
            Some(chainlist) => {
                let l2s = chainlist.l2s_of(ETHEREUM);
                tracing::info!("chainlist names {} ethereum l2s", l2s.len());
                for chain in l2s {
                    found.insert(chain.chain_id, Found::public(Family::Evm, Network::Mainnet));
                }
            }
            None => {
                tracing::warn!("chains.ethereum_l2 is on, but chainlist is not there to name them")
            }
        }
    }
    if config.chains.tron {
        found.insert(
            tron::TRON_MAINNET,
            Found::public(Family::Tron, Network::Mainnet),
        );
    }
}

fn testnets(config: &Config, chainlist: Option<&Chainlist>, found: &mut BTreeMap<u64, Found>) {
    for chain_id in ETHEREUM_TESTNETS {
        found.insert(chain_id, Found::public(Family::Evm, Network::Testnet));
    }
    if config.chains.tron {
        for chain_id in tron::TRON_TESTNETS {
            found.insert(chain_id, Found::public(Family::Tron, Network::Testnet));
        }
    }
    if config.chains.ethereum_l2
        && let Some(chainlist) = chainlist
    {
        let parents: Vec<u64> = std::iter::once(ETHEREUM).chain(ETHEREUM_TESTNETS).collect();
        let l2s = chainlist.testnet_l2s_of(&parents);
        tracing::info!("chainlist names {} ethereum testnet l2s", l2s.len());
        for chain in l2s {
            found
                .entry(chain.chain_id)
                .or_insert_with(|| Found::public(Family::Evm, Network::Testnet));
        }
    }
}

async fn devnets(
    config: &Config,
    chainlist: Option<&Chainlist>,
    http: &Client,
    found: &mut BTreeMap<u64, Found>,
) {
    let devnet = &config.chains.devnet;
    let asked = devnet
        .evm_rpc
        .iter()
        .map(|url| (Family::Evm, url))
        .chain(devnet.tron_rpc.iter().map(|url| (Family::Tron, url)));

    for (family, raw) in asked {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let url = match Url::parse(raw) {
            Ok(url) => family.dialect().node_url(url),
            Err(error) => {
                tracing::warn!(
                    "chains.devnet has an invalid {} rpc {raw}: {error}",
                    family.name()
                );
                continue;
            }
        };

        let chain_id = match served_chain(http, &url, family.dialect()).await {
            Ok(chain_id) => chain_id,
            Err(reason) => {
                tracing::info!(
                    "no {} devnet at {raw}, leaving it out: {reason}",
                    family.name()
                );
                continue;
            }
        };

        let taken = chain_id == config.chains.default
            || config.network(chain_id).is_some()
            || found.contains_key(&chain_id);
        let planned_devnet = config
            .network(chain_id)
            .and_then(|network| network.kind)
            .map(Network::from)
            .or(found.get(&chain_id).map(|found| found.network))
            .unwrap_or_else(|| kind_of(chainlist, chain_id))
            == Network::Devnet;
        if taken && !planned_devnet {
            tracing::warn!(
                "the devnet at {raw} answers as chain {chain_id}, which is served as a real network, leaving it out; give the devnet its own chain id"
            );
            continue;
        }

        tracing::info!("{} devnet at {raw} is chain {chain_id}", family.name());
        let entry = found.entry(chain_id).or_insert_with(|| Found {
            family,
            network: Network::Devnet,
            own: Vec::new(),
            name: Some(devnet_name(family, chain_id)),
        });
        if !entry.own.contains(&url) {
            entry.own.push(url);
        }
    }
}

fn devnet_name(family: Family, chain_id: u64) -> String {
    match (family, networks::known_name(chain_id)) {
        (_, Some(name)) => name.to_owned(),
        (Family::Tron, None) => "Tron devnet".to_owned(),
        (Family::Evm, None) => format!("Devnet {chain_id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(yaml: &str) -> Config {
        Config::from_yaml(yaml).unwrap()
    }

    #[tokio::test]
    async fn testnets_join_their_families_only_when_asked() {
        let http = Client::new();
        let base = "chains:\n  tron: true\n  networks:\n    1:\n      rpc:\n        additionally_rpc: [https://eth.example]\n";

        let off = plan(&config(base), None, &http).await;
        assert!(!off.contains_key(&networks::SEPOLIA));
        assert!(!off.contains_key(&tron::TRON_NILE));

        let on = plan(&config(&format!("{base}  testnets: true\n")), None, &http).await;
        assert_eq!(on[&networks::SEPOLIA].network, Network::Testnet);
        assert_eq!(on[&networks::HOODI].family, Family::Evm);
        assert_eq!(on[&tron::TRON_NILE].family, Family::Tron);
        assert_eq!(on[&tron::TRON_SHASTA].network, Network::Testnet);
        assert_eq!(on[&tron::TRON_MAINNET].network, Network::Mainnet);
        assert_eq!(on[&1].network, Network::Mainnet);
    }

    #[tokio::test]
    async fn a_configured_network_can_be_a_tron_devnet() {
        let plan = plan(
            &config(
                "chains:\n  networks:\n    1:\n      rpc:\n        additionally_rpc: [https://eth.example]\n    12345:\n      family: tron\n      kind: devnet\n      rpc:\n        additionally_rpc: [http://127.0.0.1:9090]\n",
            ),
            None,
            &Client::new(),
        )
        .await;

        assert_eq!(plan[&12345].family, Family::Tron);
        assert_eq!(plan[&12345].network, Network::Devnet);
    }

    #[tokio::test]
    async fn a_missing_devnet_is_left_out() {
        let plan = plan(
            &config(
                "chains:\n  devnets: true\n  devnet:\n    evm_rpc: [http://127.0.0.1:9]\n    tron_rpc: []\n  networks:\n    1:\n      rpc:\n        additionally_rpc: [https://eth.example]\n",
            ),
            None,
            &Client::new(),
        )
        .await;

        assert_eq!(plan.keys().copied().collect::<Vec<_>>(), [1]);
    }
}
