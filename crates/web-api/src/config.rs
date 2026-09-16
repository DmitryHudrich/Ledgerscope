mod interpolate;

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

use adapters::eth::DEFAULT_RATE_LIMIT_RPS;

pub const CONFIG_PATH_VAR: &str = "LEDGERSCOPE_CONFIG";

const DEFAULT_CONFIG_PATH: &str = "config.yaml";
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";
const DEFAULT_LOG_FILTER: &str = "info";
const DEFAULT_RPC_PROXY: &str = "socks5h://127.0.0.1:2080";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub server: ServerConfig,
    pub log: LogConfig,
    pub eth: EthConfig,
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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EthRpcConfig {
    pub url: String,
    pub proxy: String,
    pub rps: u32,
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
            proxy: DEFAULT_RPC_PROXY.to_owned(),
            rps: DEFAULT_RATE_LIMIT_RPS,
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let mut config = match config_path() {
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
        if let Some(url) = env_set("ETH_RPC_URL") {
            self.eth.rpc.url = url;
        }
        if let Ok(proxy) = std::env::var("ETH_RPC_PROXY") {
            self.eth.rpc.proxy = proxy;
        }
        if let Some(rps) = env_set("ETH_RPC_RPS") {
            self.eth.rpc.rps = rps.parse().context("ETH_RPC_RPS must be a number")?;
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

        Ok(())
    }
}

fn config_path() -> Option<PathBuf> {
    match env_set(CONFIG_PATH_VAR) {
        Some(path) => Some(PathBuf::from(path)),
        None => Some(PathBuf::from(DEFAULT_CONFIG_PATH)).filter(|path| path.is_file()),
    }
}

fn env_set(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        let config = Config::from_yaml("eth:\n  rpc:\n    url: https://rpc.example\n").unwrap();

        assert_eq!(config.server.bind_addr, DEFAULT_BIND_ADDR);
        assert_eq!(config.log.filter, DEFAULT_LOG_FILTER);
        assert_eq!(config.eth.rpc.proxy, DEFAULT_RPC_PROXY);
        assert_eq!(config.eth.rpc.rps, DEFAULT_RATE_LIMIT_RPS);
        assert_eq!(config.eth.rpc.url, "https://rpc.example");
    }

    #[test]
    fn an_empty_config_is_all_defaults() {
        let config = Config::from_yaml("").unwrap();

        assert_eq!(config.server.bind_addr, DEFAULT_BIND_ADDR);
        assert!(config.eth.rpc.url.is_empty());
    }

    #[test]
    fn an_empty_proxy_means_a_direct_connection() {
        let config = Config::from_yaml("eth:\n  rpc:\n    proxy: \"\"\n").unwrap();

        assert!(config.eth.rpc.proxy.is_empty());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let error = Config::from_yaml("server:\n  bind_address: 0.0.0.0:3000\n").unwrap_err();

        assert!(format!("{error:#}").contains("bind_address"));
    }

    #[test]
    fn a_url_is_required() {
        let mut config = Config::from_yaml("server:\n  bind_addr: 0.0.0.0:3000\n").unwrap();

        assert!(config.validate().is_err());

        config.eth.rpc.url = "https://rpc.example".to_owned();

        assert!(config.validate().is_ok());
    }
}
