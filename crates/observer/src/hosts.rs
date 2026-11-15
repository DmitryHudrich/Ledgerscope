use std::collections::HashMap;

use crate::config::HostGroup;

pub struct Match {
    pub chain_id: u64,
}

pub struct HostMatcher {
    exact: HashMap<String, u64>,
    suffixes: Vec<(String, u64)>,
}

impl HostMatcher {
    pub fn build(groups: &[HostGroup]) -> Self {
        let mut exact: HashMap<String, u64> = HashMap::new();
        let mut suffixes: Vec<(String, u64)> = Vec::new();
        for group in groups {
            for pattern in &group.hosts {
                let pattern = pattern.trim().to_ascii_lowercase();
                if pattern.is_empty() {
                    continue;
                }
                if let Some(rest) = pattern.strip_prefix("*.") {
                    suffixes.push((format!(".{rest}"), group.chain_id));
                    exact.entry(rest.to_owned()).or_insert(group.chain_id);
                } else {
                    exact.entry(pattern).or_insert(group.chain_id);
                }
            }
        }
        Self { exact, suffixes }
    }

    pub fn len(&self) -> usize {
        self.exact.len() + self.suffixes.len()
    }

    pub fn find(&self, host: &str) -> Option<Match> {
        let host = host.trim().to_ascii_lowercase();
        if let Some(chain_id) = self.exact.get(&host) {
            return Some(Match { chain_id: *chain_id });
        }
        for (suffix, chain_id) in &self.suffixes {
            if host.ends_with(suffix.as_str()) {
                return Some(Match { chain_id: *chain_id });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(family: &str, chain_id: u64, hosts: &[&str]) -> HostGroup {
        HostGroup {
            family: family.to_owned(),
            chain_id,
            hosts: hosts.iter().map(|host| host.to_string()).collect(),
        }
    }

    #[test]
    fn exact_and_wildcard_hosts_resolve_to_a_chain() {
        let matcher = HostMatcher::build(&[
            group("eth", 1, &["*.infura.io", "cloudflare-eth.com"]),
            group("tron", 728_126_428, &["*.trongrid.io", "api.trongrid.io"]),
        ]);

        assert_eq!(matcher.find("polygon-mainnet.infura.io").unwrap().chain_id, 1);
        assert_eq!(matcher.find("cloudflare-eth.com").unwrap().chain_id, 1);
        assert_eq!(matcher.find("api.nile.trongrid.io").unwrap().chain_id, 728_126_428);
        assert_eq!(matcher.find("trongrid.io").unwrap().chain_id, 728_126_428);
        assert!(matcher.find("example.com").is_none());
    }

    #[test]
    fn the_first_group_wins_for_shared_providers() {
        let matcher = HostMatcher::build(&[
            group("eth", 1, &["rpc.ankr.com"]),
            group("tron", 728_126_428, &["rpc.ankr.com"]),
        ]);

        assert_eq!(matcher.find("rpc.ankr.com").unwrap().chain_id, 1);
    }
}
