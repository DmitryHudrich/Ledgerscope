mod interpolate;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use adapters::eth::{DEFAULT_CHAINLIST_URL, DEFAULT_RATE_LIMIT_RPS, ETHEREUM};
use anyhow::Context;
use serde::Deserialize;

const DEFAULT_CONFIG_PATH: &str = "config.yaml";
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";
const DEFAULT_LOG_FILTER: &str = "info";
const DEFAULT_CLICKHOUSE_URL: &str = "http://127.0.0.1:8123";
const DEFAULT_CLICKHOUSE_DATABASE: &str = "ledgerscope";
const DEFAULT_CLICKHOUSE_USER: &str = "ledgerscope";
const DEFAULT_REDIS_URL: &str = "redis://127.0.0.1:6379";
const DEFAULT_MAX_ROOTS: usize = 64;
const DEFAULT_MAX_DEPTH: u32 = 5;
const DEFAULT_MAX_NODES: usize = 5_000;
const DEFAULT_MAX_EDGES: usize = 20_000;
const DEFAULT_MAX_BLOCKS: u64 = 100_000;
const DEFAULT_WINDOW_BLOCKS: u64 = 96;
const DEFAULT_BATCH_BLOCKS: usize = 10;
const DEFAULT_AWS_PUBLIC_URL: &str = "https://aws-public-blockchain.s3.us-east-2.amazonaws.com/";
const DEFAULT_AWS_PUBLIC_PREFIX: &str = "v1.0/eth";
const DEFAULT_AWS_PUBLIC_CHAIN: u64 = 1;
const DEFAULT_AWS_PUBLIC_CACHE: &str = "cache/aws-public";
const DEFAULT_AWS_PUBLIC_KEEP_DAYS: usize = 3;
const DEFAULT_AWS_PUBLIC_WINDOW: u64 = 1_200;
const DEFAULT_RESCAN_INTERVAL_SECS: u64 = 1800;
const DEFAULT_CHAINLIST_MAX_NODES: usize = 16;
const DEFAULT_POSTGRES_URL: &str = "postgres://ledgerscope:ledgerscope@127.0.0.1:5432/ledgerscope";
const DEFAULT_POSTGRES_MAX_CONNECTIONS: u32 = 10;
const DEFAULT_SESSION_TTL_DAYS: u32 = 30;
const DEFAULT_MAX_CANVAS_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_PERSONAL_RPC_MAX_NODES: usize = 8;
const DEFAULT_OBSERVER_WINDOW_SECS: u64 = 60;
const DEFAULT_OBSERVER_SKEW_SECS: u64 = 5;
const DEFAULT_EVM_DEVNET_RPC: &str = "http://127.0.0.1:8545";
const DEFAULT_TRON_DEVNET_RPC: &str = "http://127.0.0.1:9090";

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub proxy: String,
    pub no_proxy: String,
    pub server: ServerConfig,
    pub log: LogConfig,
    pub labels: LabelsConfig,
    pub rpc: RpcConfig,
    pub chains: ChainsConfig,
    pub storage: StorageConfig,
    pub graph: GraphConfig,
    pub indexer: IndexerConfig,
    pub accounts: AccountsConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            proxy: String::new(),
            no_proxy: String::new(),
            server: ServerConfig::default(),
            log: LogConfig::default(),
            labels: LabelsConfig::default(),
            rpc: RpcConfig::default(),
            chains: ChainsConfig::default(),
            storage: StorageConfig::default(),
            graph: GraphConfig::default(),
            indexer: IndexerConfig::default(),
            accounts: AccountsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AccountsConfig {
    pub enabled: bool,
    pub postgres_url: String,
    pub max_connections: u32,
    pub session_ttl_days: u32,
    pub registration: bool,
    pub cookie_secure: bool,
    pub max_canvas_bytes: usize,
    pub personal_rpc: PersonalRpcConfig,
    pub observer: ObserverConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ObserverConfig {
    pub enabled: bool,
    pub token: String,
    pub window_secs: u64,
    pub skew_secs: u64,
}

impl Default for ObserverConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: String::new(),
            window_secs: DEFAULT_OBSERVER_WINDOW_SECS,
            skew_secs: DEFAULT_OBSERVER_SKEW_SECS,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PersonalRpcConfig {
    pub enabled: bool,
    pub max_nodes_per_chain: usize,
    pub allow_private_hosts: bool,
}

impl Default for PersonalRpcConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_nodes_per_chain: DEFAULT_PERSONAL_RPC_MAX_NODES,
            allow_private_hosts: false,
        }
    }
}

impl Default for AccountsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            postgres_url: DEFAULT_POSTGRES_URL.to_owned(),
            max_connections: DEFAULT_POSTGRES_MAX_CONNECTIONS,
            session_ttl_days: DEFAULT_SESSION_TTL_DAYS,
            registration: true,
            cookie_secure: false,
            max_canvas_bytes: DEFAULT_MAX_CANVAS_BYTES,
            personal_rpc: PersonalRpcConfig::default(),
            observer: ObserverConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IndexerConfig {
    pub window_blocks: u64,
    pub parallel_windows: usize,
    pub batch_blocks: usize,
    pub aws_public: AwsPublicSection,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            window_blocks: DEFAULT_WINDOW_BLOCKS,
            parallel_windows: 0,
            batch_blocks: DEFAULT_BATCH_BLOCKS,
            aws_public: AwsPublicSection::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AwsPublicSection {
    pub enabled: bool,
    pub base_url: String,
    pub prefix: String,
    pub chains: Vec<u64>,
    pub cache_dir: String,
    pub keep_days: usize,
    pub window_blocks: u64,
}

impl Default for AwsPublicSection {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: DEFAULT_AWS_PUBLIC_URL.to_owned(),
            prefix: DEFAULT_AWS_PUBLIC_PREFIX.to_owned(),
            chains: vec![DEFAULT_AWS_PUBLIC_CHAIN],
            cache_dir: DEFAULT_AWS_PUBLIC_CACHE.to_owned(),
            keep_days: DEFAULT_AWS_PUBLIC_KEEP_DAYS,
            window_blocks: DEFAULT_AWS_PUBLIC_WINDOW,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GraphConfig {
    pub max_roots: usize,
    pub max_depth: u32,
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_blocks: u64,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            max_roots: DEFAULT_MAX_ROOTS,
            max_depth: DEFAULT_MAX_DEPTH,
            max_nodes: DEFAULT_MAX_NODES,
            max_edges: DEFAULT_MAX_EDGES,
            max_blocks: DEFAULT_MAX_BLOCKS,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub bind_addr: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LogConfig {
    pub filter: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LabelsConfig {
    pub etherscan_json_path: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RpcConfig {
    pub rps: Option<u32>,
    pub chainlist: Option<bool>,
    pub chainlist_max_nodes: Option<usize>,
    pub additionally_rpc: Vec<String>,
    pub exclude_rpc: Vec<String>,
    pub chainlist_url: Option<String>,
    pub rescan_interval_secs: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChainsConfig {
    pub default: u64,
    pub ethereum_l2: bool,
    pub tron: bool,
    pub testnets: bool,
    pub devnets: bool,
    pub devnet: DevnetConfig,
    pub skip: Vec<u64>,
    pub networks: BTreeMap<u64, NetworkConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DevnetConfig {
    pub evm_rpc: Vec<String>,
    pub tron_rpc: Vec<String>,
}

impl Default for DevnetConfig {
    fn default() -> Self {
        Self {
            evm_rpc: vec![DEFAULT_EVM_DEVNET_RPC.to_owned()],
            tron_rpc: vec![DEFAULT_TRON_DEVNET_RPC.to_owned()],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FamilyConfig {
    Evm,
    Tron,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkKind {
    Mainnet,
    Testnet,
    Devnet,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NetworkConfig {
    pub family: Option<FamilyConfig>,
    pub kind: Option<NetworkKind>,
    pub name: Option<String>,
    pub slug: Option<String>,
    pub native_symbol: Option<String>,
    pub native_decimals: Option<u8>,
    pub rpc: RpcConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainRpc {
    pub rps: u32,
    pub chainlist: bool,
    pub chainlist_max_nodes: usize,
    pub own: Vec<String>,
    pub exclude: Vec<String>,
}

impl Default for ChainsConfig {
    fn default() -> Self {
        Self {
            default: ETHEREUM,
            ethereum_l2: false,
            tron: false,
            testnets: false,
            devnets: false,
            devnet: DevnetConfig::default(),
            skip: Vec::new(),
            networks: BTreeMap::new(),
        }
    }
}

impl RpcConfig {
    pub fn chainlist_url(&self) -> &str {
        self.chainlist_url
            .as_deref()
            .unwrap_or(DEFAULT_CHAINLIST_URL)
    }

    pub fn rescan_interval_secs(&self) -> u64 {
        self.rescan_interval_secs
            .unwrap_or(DEFAULT_RESCAN_INTERVAL_SECS)
    }

    fn own_urls(&self) -> impl Iterator<Item = &str> {
        self.additionally_rpc
            .iter()
            .map(|url| url.trim())
            .filter(|url| !url.is_empty())
    }

    fn validate_shared(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.own_urls().next().is_none(),
            "rpc.additionally_rpc belongs to a network: move it under chains.networks.<chain_id>.rpc"
        );
        self.validate_common("rpc")
    }

    fn validate_network(&self, chain_id: u64) -> anyhow::Result<()> {
        let at = format!("chains.networks.{chain_id}.rpc");
        anyhow::ensure!(
            self.chainlist_url.is_none() && self.rescan_interval_secs.is_none(),
            "{at}: chainlist_url and rescan_interval_secs are shared, set them under rpc"
        );
        self.validate_common(&at)
    }

    fn validate_common(&self, at: &str) -> anyhow::Result<()> {
        anyhow::ensure!(self.rps != Some(0), "{at}.rps must be at least 1");
        anyhow::ensure!(
            self.chainlist_max_nodes != Some(0),
            "{at}.chainlist_max_nodes must be at least 1"
        );
        Ok(())
    }
}

impl Config {
    pub fn chain_ids(&self, discovered: impl IntoIterator<Item = u64>) -> Vec<u64> {
        let mut chain_ids: Vec<u64> = std::iter::once(self.chains.default)
            .chain(self.chains.networks.keys().copied())
            .chain(discovered)
            .filter(|chain_id| {
                *chain_id == self.chains.default || !self.chains.skip.contains(chain_id)
            })
            .collect();
        chain_ids.sort_unstable();
        chain_ids.dedup();
        chain_ids
    }

    pub fn configured(&self, chain_id: u64) -> bool {
        chain_id == self.chains.default || self.chains.networks.contains_key(&chain_id)
    }

    pub fn network(&self, chain_id: u64) -> Option<&NetworkConfig> {
        self.chains.networks.get(&chain_id)
    }

    pub fn chain_rpc(&self, chain_id: u64) -> ChainRpc {
        let shared = &self.rpc;
        let empty = RpcConfig::default();
        let own = self
            .network(chain_id)
            .map_or(&empty, |network| &network.rpc);

        ChainRpc {
            rps: own.rps.or(shared.rps).unwrap_or(DEFAULT_RATE_LIMIT_RPS),
            chainlist: own.chainlist.or(shared.chainlist).unwrap_or(false),
            chainlist_max_nodes: own
                .chainlist_max_nodes
                .or(shared.chainlist_max_nodes)
                .unwrap_or(DEFAULT_CHAINLIST_MAX_NODES),
            own: own.own_urls().map(str::to_owned).collect(),
            exclude: shared
                .exclude_rpc
                .iter()
                .chain(&own.exclude_rpc)
                .map(|rule| rule.trim().to_owned())
                .filter(|rule| !rule.is_empty())
                .collect(),
        }
    }

    pub fn needs_chainlist(&self) -> bool {
        self.chains.ethereum_l2
            || (self.chains.testnets && self.rpc.chainlist.unwrap_or(false))
            || self
                .chain_ids([])
                .into_iter()
                .any(|chain_id| self.chain_rpc(chain_id).chainlist)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StorageConfig {
    pub persist_txs: bool,
    pub clickhouse: ClickhouseConfig,
    pub redis: RedisConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClickhouseConfig {
    pub url: String,
    pub database: String,
    pub user: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RedisConfig {
    pub url: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: DEFAULT_BIND_ADDR.to_owned(),
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            filter: DEFAULT_LOG_FILTER.to_owned(),
        }
    }
}

impl Default for ClickhouseConfig {
    fn default() -> Self {
        Self {
            url: DEFAULT_CLICKHOUSE_URL.to_owned(),
            database: DEFAULT_CLICKHOUSE_DATABASE.to_owned(),
            user: DEFAULT_CLICKHOUSE_USER.to_owned(),
            password: String::new(),
        }
    }
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: DEFAULT_REDIS_URL.to_owned(),
        }
    }
}

impl Config {
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let mut config = match config_path(path) {
            Some(path) => Self::from_file(&path)?,
            None => Self::default(),
        };

        config.apply_env()?;
        config.validate()?;

        Ok(config)
    }

    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let yaml = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read the config at {}", path.display()))?;

        Self::from_yaml(&yaml).with_context(|| format!("invalid config at {}", path.display()))
    }

    pub fn from_yaml(yaml: &str) -> anyhow::Result<Self> {
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(yaml).context("the config is not valid YAML")?;

        if parsed.is_null() {
            return Ok(Self::default());
        }

        let resolved = interpolate::resolve(parsed)?;

        serde_yaml::from_value(resolved).context("the config has an unexpected shape")
    }

    fn apply_env(&mut self) -> anyhow::Result<()> {
        if let Some(bind_addr) = env_set("BIND_ADDR") {
            self.server.bind_addr = bind_addr;
        }
        if let Some(filter) = env_set("RUST_LOG") {
            self.log.filter = filter;
        }
        if let Some(proxy) = ["PROXY", "RPC_PROXY"]
            .into_iter()
            .find_map(|name| std::env::var(name).ok())
        {
            self.proxy = proxy;
        }
        if let Ok(no_proxy) = std::env::var("NO_PROXY") {
            self.no_proxy = no_proxy;
        }
        if let Some(url) = env_set("ETH_RPC_URL") {
            let own = &mut self
                .chains
                .networks
                .entry(ETHEREUM)
                .or_default()
                .rpc
                .additionally_rpc;
            if !own.contains(&url) {
                own.insert(0, url);
            }
        }
        if let Some(rps) = env_set("ETH_RPC_RPS") {
            self.rpc.rps = Some(rps.parse().context("ETH_RPC_RPS must be a number")?);
        }
        if let Some(chainlist) = env_set("RPC_CHAINLIST") {
            self.rpc.chainlist =
                Some(parse_bool(&chainlist).with_context(|| {
                    format!("RPC_CHAINLIST must be a boolean, got {chainlist}")
                })?);
        }
        if let Some(l2) = env_set("CHAINS_ETHEREUM_L2") {
            self.chains.ethereum_l2 = parse_bool(&l2)
                .with_context(|| format!("CHAINS_ETHEREUM_L2 must be a boolean, got {l2}"))?;
        }
        if let Some(tron) = env_set("CHAINS_TRON") {
            self.chains.tron = parse_bool(&tron)
                .with_context(|| format!("CHAINS_TRON must be a boolean, got {tron}"))?;
        }
        if let Some(testnets) = env_set("CHAINS_TESTNETS") {
            self.chains.testnets = parse_bool(&testnets)
                .with_context(|| format!("CHAINS_TESTNETS must be a boolean, got {testnets}"))?;
        }
        if let Some(devnets) = env_set("CHAINS_DEVNETS") {
            self.chains.devnets = parse_bool(&devnets)
                .with_context(|| format!("CHAINS_DEVNETS must be a boolean, got {devnets}"))?;
        }
        if let Some(default) = env_set("CHAINS_DEFAULT") {
            self.chains.default = default
                .parse()
                .context("CHAINS_DEFAULT must be a chain id")?;
        }
        if let Some(path) = env_set("ETHERSCAN_JSON_PATH") {
            self.labels.etherscan_json_path = path;
        }
        if let Some(persist) = env_set("STORAGE_PERSIST_TXS") {
            self.storage.persist_txs = parse_bool(&persist)
                .with_context(|| format!("STORAGE_PERSIST_TXS must be a boolean, got {persist}"))?;
        }
        if let Some(url) = env_set("CLICKHOUSE_URL") {
            self.storage.clickhouse.url = url;
        }
        if let Some(database) = env_set("CLICKHOUSE_DB") {
            self.storage.clickhouse.database = database;
        }
        if let Some(user) = env_set("CLICKHOUSE_USER") {
            self.storage.clickhouse.user = user;
        }
        if let Ok(password) = std::env::var("CLICKHOUSE_PASSWORD") {
            self.storage.clickhouse.password = password;
        }
        if let Ok(url) = std::env::var("REDIS_URL") {
            self.storage.redis.url = url;
        }
        if let Some(enabled) = env_set("ACCOUNTS_ENABLED") {
            self.accounts.enabled = parse_bool(&enabled)
                .with_context(|| format!("ACCOUNTS_ENABLED must be a boolean, got {enabled}"))?;
        }
        if let Some(url) = env_set("POSTGRES_URL") {
            self.accounts.postgres_url = url;
        }
        if let Some(registration) = env_set("AUTH_REGISTRATION") {
            self.accounts.registration = parse_bool(&registration).with_context(|| {
                format!("AUTH_REGISTRATION must be a boolean, got {registration}")
            })?;
        }
        if let Some(personal) = env_set("PERSONAL_RPC") {
            self.accounts.personal_rpc.enabled = parse_bool(&personal)
                .with_context(|| format!("PERSONAL_RPC must be a boolean, got {personal}"))?;
        }
        if let Some(secure) = env_set("AUTH_COOKIE_SECURE") {
            self.accounts.cookie_secure = parse_bool(&secure)
                .with_context(|| format!("AUTH_COOKIE_SECURE must be a boolean, got {secure}"))?;
        }
        if let Some(enabled) = env_set("OBSERVER_ENABLED") {
            self.accounts.observer.enabled = parse_bool(&enabled)
                .with_context(|| format!("OBSERVER_ENABLED must be a boolean, got {enabled}"))?;
        }
        if let Some(token) = env_set("OBSERVER_TOKEN") {
            self.accounts.observer.token = token;
        }
        if let Some(urls) = env_set("DEVNET_EVM_RPC") {
            self.chains.devnet.evm_rpc = split_urls(&urls);
        }
        if let Some(urls) = env_set("DEVNET_TRON_RPC") {
            self.chains.devnet.tron_rpc = split_urls(&urls);
        }
        if let Some(roots) = env_set("GRAPH_MAX_ROOTS") {
            self.graph.max_roots = roots.parse().context("GRAPH_MAX_ROOTS must be a number")?;
        }
        if let Some(depth) = env_set("GRAPH_MAX_DEPTH") {
            self.graph.max_depth = depth.parse().context("GRAPH_MAX_DEPTH must be a number")?;
        }
        if let Some(nodes) = env_set("GRAPH_MAX_NODES") {
            self.graph.max_nodes = nodes.parse().context("GRAPH_MAX_NODES must be a number")?;
        }
        if let Some(edges) = env_set("GRAPH_MAX_EDGES") {
            self.graph.max_edges = edges.parse().context("GRAPH_MAX_EDGES must be a number")?;
        }
        if let Some(blocks) = env_set("GRAPH_MAX_BLOCKS") {
            self.graph.max_blocks = blocks
                .parse()
                .context("GRAPH_MAX_BLOCKS must be a number")?;
        }

        Ok(())
    }

    fn validate(&self) -> anyhow::Result<()> {
        self.rpc.validate_shared()?;
        for (chain_id, network) in &self.chains.networks {
            network.rpc.validate_network(*chain_id)?;
        }
        anyhow::ensure!(
            !self.chains.skip.contains(&self.chains.default),
            "chains.default {} is listed in chains.skip",
            self.chains.default
        );

        let default = self.chain_rpc(self.chains.default);
        anyhow::ensure!(
            !default.own.is_empty() || default.chainlist,
            "chain {} has no rpc: set chains.networks.{}.rpc.additionally_rpc (or ETH_RPC_URL) or turn rpc.chainlist on",
            self.chains.default,
            self.chains.default
        );
        if self.needs_chainlist() {
            anyhow::ensure!(
                !self.rpc.chainlist_url().is_empty(),
                "rpc.chainlist_url must not be empty"
            );
        }

        anyhow::ensure!(
            !self.server.bind_addr.is_empty(),
            "server.bind_addr must not be empty"
        );

        anyhow::ensure!(
            self.graph.max_roots > 0,
            "graph.max_roots must be at least 1"
        );
        anyhow::ensure!(
            self.graph.max_depth > 0,
            "graph.max_depth must be at least 1"
        );
        anyhow::ensure!(
            self.graph.max_nodes > 0,
            "graph.max_nodes must be at least 1"
        );
        anyhow::ensure!(
            self.graph.max_edges > 0,
            "graph.max_edges must be at least 1"
        );
        anyhow::ensure!(
            self.graph.max_blocks > 0,
            "graph.max_blocks must be at least 1"
        );

        if self.accounts.enabled {
            anyhow::ensure!(
                !self.accounts.postgres_url.is_empty(),
                "accounts.postgres_url is required when accounts.enabled is on"
            );
            anyhow::ensure!(
                self.accounts.max_connections > 0,
                "accounts.max_connections must be at least 1"
            );
            anyhow::ensure!(
                self.accounts.session_ttl_days > 0,
                "accounts.session_ttl_days must be at least 1"
            );
            anyhow::ensure!(
                self.accounts.max_canvas_bytes >= 1024,
                "accounts.max_canvas_bytes must be at least 1024"
            );
            anyhow::ensure!(
                !self.accounts.personal_rpc.enabled
                    || self.accounts.personal_rpc.max_nodes_per_chain > 0,
                "accounts.personal_rpc.max_nodes_per_chain must be at least 1"
            );
        }

        if self.storage.persist_txs {
            anyhow::ensure!(
                !self.storage.clickhouse.url.is_empty(),
                "storage.clickhouse.url is required when storage.persist_txs is on"
            );
            anyhow::ensure!(
                !self.storage.clickhouse.database.is_empty(),
                "storage.clickhouse.database must not be empty"
            );
        }

        Ok(())
    }
}

fn parse_bool(value: &str) -> anyhow::Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => anyhow::bail!("{other} is not a boolean"),
    }
}

fn config_path(explicit: Option<&Path>) -> Option<PathBuf> {
    match explicit {
        Some(path) => Some(path.to_path_buf()),
        None => Some(PathBuf::from(DEFAULT_CONFIG_PATH)).filter(|path| path.is_file()),
    }
}

fn env_set(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn split_urls(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_example_config_reads_with_the_indexer_section() {
        let config = Config::from_yaml(include_str!("../../../config.example.yaml")).unwrap();

        assert_eq!(config.indexer.window_blocks, 96);
        assert_eq!(config.indexer.aws_public.chains, [1]);
        assert_eq!(config.indexer.aws_public.prefix, "v1.0/eth");
    }

    const YAML: &str = "
rpc:
  rps: 10
  chainlist: true
  exclude_rpc: [bad.example]
chains:
  ethereum_l2: true
  skip: [42170]
  networks:
    1:
      rpc:
        additionally_rpc: [https://mine.example]
        rps: 25
    8453:
      rpc:
        chainlist: false
        additionally_rpc: [https://base.example]
        exclude_rpc: [worse.example]
";

    #[test]
    fn a_network_overrides_the_shared_rpc() {
        let config = Config::from_yaml(YAML).unwrap();
        config.validate().unwrap();

        assert_eq!(
            config.chain_rpc(1),
            ChainRpc {
                rps: 25,
                chainlist: true,
                chainlist_max_nodes: DEFAULT_CHAINLIST_MAX_NODES,
                own: vec!["https://mine.example".to_owned()],
                exclude: vec!["bad.example".to_owned()],
            }
        );

        let base = config.chain_rpc(8453);
        assert_eq!(base.rps, 10);
        assert!(!base.chainlist);
        assert_eq!(base.exclude, ["bad.example", "worse.example"]);
    }

    #[test]
    fn unconfigured_chains_fall_back_to_the_shared_rpc() {
        let config = Config::from_yaml(YAML).unwrap();
        let arbitrum = config.chain_rpc(42161);

        assert_eq!(arbitrum.rps, 10);
        assert!(arbitrum.chainlist);
        assert!(arbitrum.own.is_empty());
    }

    #[test]
    fn skipped_chains_stay_out_but_the_default_never_does() {
        let config = Config::from_yaml(YAML).unwrap();

        assert_eq!(config.chain_ids([42161, 42170, 1]), [1, 8453, 42161]);
    }

    #[test]
    fn own_rpcs_do_not_belong_to_every_chain() {
        let config =
            Config::from_yaml("rpc:\n  additionally_rpc: [https://mine.example]\n").unwrap();

        assert!(config.validate().is_err());
    }
}
