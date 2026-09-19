mod interpolate;

use std::path::{Path, PathBuf};

use adapters::eth::DEFAULT_RATE_LIMIT_RPS;
use anyhow::Context;
use serde::Deserialize;

const DEFAULT_CONFIG_PATH: &str = "config.yaml";
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";
const DEFAULT_LOG_FILTER: &str = "info";
const DEFAULT_RPC_PROXY: &str = "socks5h://127.0.0.1:2080";
const DEFAULT_CLICKHOUSE_URL: &str = "http://127.0.0.1:8123";
const DEFAULT_CLICKHOUSE_DATABASE: &str = "ledgerscope";
const DEFAULT_CLICKHOUSE_USER: &str = "ledgerscope";
const DEFAULT_REDIS_URL: &str = "redis://127.0.0.1:6379";
const DEFAULT_MAX_ROOTS: usize = 64;
const DEFAULT_MAX_DEPTH: u32 = 5;
const DEFAULT_MAX_NODES: usize = 5_000;
const DEFAULT_MAX_EDGES: usize = 20_000;
const DEFAULT_MAX_BLOCKS: u64 = 100_000;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub proxy: String,
    pub server: ServerConfig,
    pub log: LogConfig,
    pub eth: EthConfig,
    pub storage: StorageConfig,
    pub graph: GraphConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            proxy: DEFAULT_RPC_PROXY.to_owned(),
            server: ServerConfig::default(),
            log: LogConfig::default(),
            eth: EthConfig::default(),
            storage: StorageConfig::default(),
            graph: GraphConfig::default(),
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
pub struct EthConfig {
    pub rpc: EthRpcConfig,
    pub etherscan_json_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EthRpcConfig {
    pub url: String,
    pub rps: u32,
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

impl Default for EthRpcConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            rps: DEFAULT_RATE_LIMIT_RPS,
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
        if let Ok(proxy) = std::env::var("RPC_PROXY") {
            self.proxy = proxy;
        }
        if let Some(url) = env_set("ETH_RPC_URL") {
            self.eth.rpc.url = url;
        }
        if let Some(rps) = env_set("ETH_RPC_RPS") {
            self.eth.rpc.rps = rps.parse().context("ETH_RPC_RPS must be a number")?;
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
        anyhow::ensure!(
            !self.eth.rpc.url.is_empty(),
            "eth.rpc.url is required: set it in the config or as ETH_RPC_URL"
        );
        anyhow::ensure!(self.eth.rpc.rps > 0, "eth.rpc.rps must be at least 1");

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
