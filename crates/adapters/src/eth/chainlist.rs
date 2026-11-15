use std::{
    collections::HashMap,
    io,
    sync::{Arc, RwLock},
    time::Duration,
};

use reqwest::{Client, Url};
use serde::{Deserialize, de::IgnoredAny};
use serde_json::Value;

pub const DEFAULT_CHAINLIST_URL: &str = "https://chainlist.org/rpcs.json";
pub const ETHEREUM: u64 = 1;

const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const DEPRECATED: &str = "deprecated";

const UNMARKED_ETHEREUM_L2S: &[u64] = &[
    10, 8453, 130, 7777777, 57073, 1135, 34443, 252, 1923, 5330, 185, 8333, 1750, 169, 167000, 4326,
];

#[derive(Debug, Clone)]
pub struct ChainInfo {
    pub chain_id: u64,
    pub name: String,
    pub slug: String,
    pub native_symbol: String,
    pub native_decimals: u8,
    pub rpc: Vec<Url>,
    pub l2_of: Option<u64>,
    testnet: bool,
    deprecated: bool,
}

impl ChainInfo {
    pub fn testnet(&self) -> bool {
        self.testnet
    }
}

#[derive(Deserialize)]
struct RawChain {
    #[serde(rename = "chainId")]
    chain_id: u64,
    name: String,
    #[serde(rename = "shortName", default)]
    short_name: String,
    #[serde(rename = "chainSlug", default)]
    chain_slug: Option<String>,
    #[serde(rename = "nativeCurrency", default)]
    native_currency: Option<RawCurrency>,
    #[serde(default)]
    rpc: Vec<RawRpc>,
    #[serde(default)]
    parent: Option<RawParent>,
    #[serde(rename = "isTestnet", default)]
    is_testnet: bool,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Deserialize)]
struct RawCurrency {
    symbol: String,
    #[serde(default)]
    decimals: Option<u8>,
}

#[derive(Deserialize)]
struct RawParent {
    #[serde(rename = "type")]
    kind: String,
    chain: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawRpc {
    Plain(String),
    Detailed { url: String },
    Other(IgnoredAny),
}

pub struct Chainlist {
    http: Client,
    source: Url,
    chains: RwLock<Arc<HashMap<u64, ChainInfo>>>,
}

impl Chainlist {
    pub async fn load(http: Client, source: Url) -> io::Result<Arc<Self>> {
        let chainlist = Arc::new(Self {
            http,
            source,
            chains: RwLock::new(Arc::new(HashMap::new())),
        });

        chainlist.refresh().await?;
        Ok(chainlist)
    }

    pub async fn refresh(&self) -> io::Result<()> {
        let raw: Vec<Value> = self
            .http
            .get(self.source.clone())
            .timeout(FETCH_TIMEOUT)
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(|error| io::Error::other(format!("chainlist is not reachable: {error}")))?
            .json()
            .await
            .map_err(|error| io::Error::other(format!("chainlist sent something odd: {error}")))?;

        let chains = parse(raw);
        tracing::info!("chainlist knows {} chains", chains.len());

        *self.chains.write().expect("the lock is never poisoned") = Arc::new(chains);
        Ok(())
    }

    pub fn chain(&self, chain_id: u64) -> Option<ChainInfo> {
        self.snapshot().get(&chain_id).cloned()
    }

    pub fn rpc_urls(&self, chain_id: u64) -> Vec<Url> {
        self.chain(chain_id)
            .map(|chain| chain.rpc)
            .unwrap_or_default()
    }

    pub fn l2s_of(&self, parent: u64) -> Vec<ChainInfo> {
        self.l2s(|chain| !chain.testnet && chain.l2_of == Some(parent))
    }

    pub fn testnet_l2s_of(&self, parents: &[u64]) -> Vec<ChainInfo> {
        self.l2s(|chain| chain.testnet && chain.l2_of.is_some_and(|of| parents.contains(&of)))
    }

    fn l2s(&self, wanted: impl Fn(&ChainInfo) -> bool) -> Vec<ChainInfo> {
        let mut l2s: Vec<ChainInfo> = self
            .snapshot()
            .values()
            .filter(|chain| !chain.deprecated && !chain.rpc.is_empty() && wanted(chain))
            .cloned()
            .collect();
        l2s.sort_by_key(|chain| chain.chain_id);
        l2s
    }

    fn snapshot(&self) -> Arc<HashMap<u64, ChainInfo>> {
        self.chains
            .read()
            .expect("the lock is never poisoned")
            .clone()
    }
}

fn parse(raw: Vec<Value>) -> HashMap<u64, ChainInfo> {
    raw.into_iter()
        .filter_map(|value| serde_json::from_value::<RawChain>(value).ok())
        .map(|chain| {
            let info = info(chain);
            (info.chain_id, info)
        })
        .collect()
}

fn info(chain: RawChain) -> ChainInfo {
    let l2_of = chain
        .parent
        .as_ref()
        .filter(|parent| parent.kind == "L2")
        .and_then(|parent| parent.chain.strip_prefix("eip155-"))
        .and_then(|parent| parent.parse().ok())
        .or_else(|| {
            UNMARKED_ETHEREUM_L2S
                .contains(&chain.chain_id)
                .then_some(ETHEREUM)
        });

    let slug = chain
        .chain_slug
        .filter(|slug| !slug.trim().is_empty())
        .unwrap_or(chain.short_name);

    ChainInfo {
        chain_id: chain.chain_id,
        slug: slugify(&slug),
        native_decimals: chain
            .native_currency
            .as_ref()
            .and_then(|currency| currency.decimals)
            .unwrap_or(18),
        native_symbol: chain
            .native_currency
            .map(|currency| currency.symbol)
            .unwrap_or_default(),
        rpc: chain
            .rpc
            .into_iter()
            .filter_map(|rpc| match rpc {
                RawRpc::Plain(url) | RawRpc::Detailed { url } => usable(&url),
                RawRpc::Other(_) => None,
            })
            .collect(),
        l2_of,
        testnet: chain.is_testnet,
        deprecated: chain.status.as_deref() == Some(DEPRECATED),
        name: chain.name,
    }
}

pub fn slugify(raw: &str) -> String {
    raw.trim()
        .to_lowercase()
        .split(|c: char| c.is_whitespace() || c == '_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn usable(raw: &str) -> Option<Url> {
    if raw.contains('$') || raw.contains('{') {
        return None;
    }

    Url::parse(raw.trim())
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn skips_templated_and_websocket_urls() {
        assert!(usable("https://mainnet.infura.io/v3/${INFURA_API_KEY}").is_none());
        assert!(usable("wss://eth.example").is_none());
        assert!(usable("https://eth.example/rpc").is_some());
    }

    #[test]
    fn finds_marked_and_unmarked_l2s_but_not_testnets() {
        let chains = parse(vec![
            json!({"chainId": 1, "name": "Ethereum", "shortName": "eth", "rpc": ["https://eth.example"]}),
            json!({"chainId": 42161, "name": "Arbitrum One", "shortName": "arb1", "chainSlug": "arbitrum",
                   "rpc": [{"url": "https://arb.example", "tracking": "none"}, 42],
                   "parent": {"type": "L2", "chain": "eip155-1"}}),
            json!({"chainId": 8453, "name": "Base", "shortName": "base", "rpc": ["https://base.example"]}),
            json!({"chainId": 421614, "name": "Arbitrum Sepolia", "shortName": "arb-sep", "isTestnet": true,
                   "rpc": ["https://sep.example"], "parent": {"type": "L2", "chain": "eip155-1"}}),
            json!({"chainId": 84532, "name": "Base Sepolia", "shortName": "basesep", "isTestnet": true,
                   "rpc": ["https://base-sep.example"], "parent": {"type": "L2", "chain": "eip155-11155111"}}),
            json!({"chainId": 999999, "name": "Old Sepolia L2", "shortName": "old", "isTestnet": true, "status": "deprecated",
                   "rpc": ["https://old.example"], "parent": {"type": "L2", "chain": "eip155-11155111"}}),
            json!({"chainId": "broken"}),
        ]);
        let chainlist = Chainlist {
            http: Client::new(),
            source: Url::parse(DEFAULT_CHAINLIST_URL).unwrap(),
            chains: RwLock::new(Arc::new(chains)),
        };

        let l2s: Vec<_> = chainlist
            .l2s_of(ETHEREUM)
            .into_iter()
            .map(|chain| chain.slug)
            .collect();
        assert_eq!(l2s, ["base", "arbitrum"]);

        let testnet_l2s: Vec<_> = chainlist
            .testnet_l2s_of(&[ETHEREUM, 11_155_111])
            .into_iter()
            .map(|chain| chain.chain_id)
            .collect();
        assert_eq!(testnet_l2s, [84532, 421614]);
        assert_eq!(chainlist.rpc_urls(42161).len(), 1);
    }

    #[test]
    fn slugs_are_url_friendly() {
        assert_eq!(slugify("Arbitrum Nova"), "arbitrum-nova");
        assert_eq!(slugify("op_bnb"), "op-bnb");
    }
}
