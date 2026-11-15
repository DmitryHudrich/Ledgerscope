use serde::Deserialize;

fn default_port() -> u16 {
    443
}

fn default_filter() -> String {
    "tcp port 443".to_owned()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Sni,
    Proxy,
    Sniff,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub listen: String,
    pub api: String,
    pub token: String,
    #[serde(default = "default_port")]
    pub default_port: u16,
    #[serde(default)]
    pub min_confidence: f64,
    #[serde(default)]
    pub groups: Vec<HostGroup>,
    #[cfg_attr(not(feature = "pcap"), allow(dead_code))]
    #[serde(default)]
    pub sniff: SniffConfig,
}

#[cfg_attr(not(feature = "pcap"), allow(dead_code))]
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SniffConfig {
    #[serde(default)]
    pub interface: String,
    #[serde(default = "default_filter")]
    pub filter: String,
}

impl Default for SniffConfig {
    fn default() -> Self {
        Self {
            interface: String::new(),
            filter: default_filter(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostGroup {
    pub family: String,
    pub chain_id: u64,
    pub hosts: Vec<String>,
}
