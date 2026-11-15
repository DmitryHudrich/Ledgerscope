use std::{
    collections::{BTreeMap, HashMap},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::Arc,
};

use application::account::RpcNodeProbe;
use reqwest::{Client, Url};

use crate::eth::rpc::{RpcFleet, pool::probe};

tokio::task_local! {
    static PERSONAL: Arc<PersonalNodes>;
}

pub const PERSONAL_LABEL: &str = "personal node";

#[derive(Debug, Default)]
pub struct PersonalNodes {
    by_chain: HashMap<u64, Vec<Url>>,
}

impl PersonalNodes {
    pub fn new(urls: BTreeMap<u64, Vec<String>>) -> Self {
        let by_chain = urls
            .into_iter()
            .map(|(chain_id, urls)| {
                let parsed: Vec<Url> = urls
                    .iter()
                    .filter_map(|url| match Url::parse(url) {
                        Ok(url) => Some(url),
                        Err(error) => {
                            tracing::warn!(
                                "skipping a stored personal rpc of chain {chain_id}: {error}"
                            );
                            None
                        }
                    })
                    .collect();
                (chain_id, parsed)
            })
            .filter(|(_, urls)| !urls.is_empty())
            .collect();
        Self { by_chain }
    }

    pub fn is_empty(&self) -> bool {
        self.by_chain.is_empty()
    }

    pub async fn scope<F: Future>(self, work: F) -> F::Output {
        if self.is_empty() {
            work.await
        } else {
            PERSONAL.scope(Arc::new(self), work).await
        }
    }
}

pub(crate) fn current(chain_id: u64) -> Vec<Url> {
    PERSONAL
        .try_with(|personal| personal.by_chain.get(&chain_id).cloned())
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub struct PersonalProbe {
    fleet: Arc<RpcFleet>,
    http: Client,
    allow_private: bool,
}

impl PersonalProbe {
    pub fn new(fleet: Arc<RpcFleet>, http: Client, allow_private: bool) -> Self {
        Self {
            fleet,
            http,
            allow_private,
        }
    }
}

#[async_trait::async_trait]
impl RpcNodeProbe for PersonalProbe {
    async fn probe(&self, chain_id: u64, raw: &str) -> Result<String, String> {
        let url = Url::parse(raw).map_err(|error| format!("{raw} is not a valid URL: {error}"))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("the node must speak http or https".to_owned());
        }
        let host = url.host_str().ok_or("the URL has no host")?;
        if !self.allow_private && private(host) {
            return Err("private and local addresses are not allowed here".to_owned());
        }

        let pool = self
            .fleet
            .pool(chain_id)
            .ok_or_else(|| format!("chain {chain_id} is not served here"))?;
        let dialect = pool.dialect();
        let node_url = dialect.node_url(url);

        probe::one(&self.http, &node_url, chain_id, dialect)
            .await
            .map_err(|reason| format!("the node did not pass the check: {reason}"))?;

        Ok(node_url.as_str().trim_end_matches('/').to_owned())
    }
}

fn private(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    match bare.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => private_v4(ip),
        Ok(IpAddr::V6(ip)) => private_v6(ip),
        Err(_) => {
            let domain = bare.trim_end_matches('.').to_ascii_lowercase();
            domain == "localhost"
                || [".localhost", ".local", ".internal", ".lan", ".home.arpa"]
                    .iter()
                    .any(|suffix| domain.ends_with(suffix))
                || !domain.contains('.')
        }
    }
}

fn private_v4(ip: Ipv4Addr) -> bool {
    let [first, second, ..] = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || (first == 100 && (64..128).contains(&second))
}

fn private_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return private_v4(v4);
    }
    let first = ip.segments()[0];
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::*;

    fn private_url(raw: &str) -> bool {
        private(Url::parse(raw).unwrap().host_str().unwrap())
    }

    #[test]
    fn local_and_private_hosts_are_told_apart_from_public_ones() {
        assert!(private_url("http://127.0.0.1:8545"));
        assert!(private_url("http://localhost:8123"));
        assert!(private_url("http://clickhouse:8123"));
        assert!(private_url("http://10.1.2.3"));
        assert!(private_url("http://192.168.0.10/rpc"));
        assert!(private_url("http://100.64.0.1"));
        assert!(private_url("http://169.254.169.254/latest"));
        assert!(private_url("http://[::1]:8545"));
        assert!(private_url("http://[fd00::1]"));
        assert!(private_url("http://[::ffff:127.0.0.1]"));
        assert!(private_url("http://node.internal"));

        assert!(!private_url("https://eth.llamarpc.com"));
        assert!(!private_url("https://8.8.8.8"));
        assert!(!private_url("https://[2606:4700::1111]"));
    }

    #[test]
    fn stored_nodes_land_on_their_chain_and_broken_ones_are_dropped() {
        let nodes = PersonalNodes::new(BTreeMap::from([
            (
                1,
                vec![
                    "https://mine.example/rpc".to_owned(),
                    "not a url".to_owned(),
                ],
            ),
            (10, vec!["also not".to_owned()]),
        ]));

        assert_eq!(nodes.by_chain.len(), 1);
        assert_eq!(nodes.by_chain[&1].len(), 1);
    }

    #[tokio::test]
    async fn the_nodes_are_seen_only_inside_the_scope() {
        let nodes = PersonalNodes::new(BTreeMap::from([(
            1,
            vec!["https://mine.example".to_owned()],
        )]));

        assert!(current(1).is_empty());
        let inside = nodes
            .scope(async { (current(1).len(), current(10).len()) })
            .await;
        assert_eq!(inside, (1, 0));
        assert!(current(1).is_empty());
    }
}
