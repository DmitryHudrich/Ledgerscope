pub(crate) mod probe;
mod retry;

use std::{
    collections::{HashMap, HashSet},
    io,
    sync::{
        Arc, Mutex, OnceLock, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use alloy::{
    rpc::json_rpc::{RequestPacket, ResponsePacket},
    transports::{TransportError, TransportErrorKind, TransportFut, http::Http},
};
use reqwest::{Client, Url};
use serde_json::Value;
use tokio::{sync::Semaphore, time::Instant};
use tower::Service;

use crate::eth::{
    chainlist::Chainlist,
    rate_limiter::RateLimiter,
    rpc::monitor::{CallOutcome, CallRecord, RpcMonitor, redact},
    rpc::personal::{self, PERSONAL_LABEL},
    rpc::pool::{
        probe::{Candidate, Probed},
        retry::{
            Retry, RetryKind, backoff, failed_response, http_answer, http_unreachable, retryable,
        },
    },
};

const MAX_ATTEMPTS: u32 = 5;
const MAX_HEAD_LAG: u64 = 5;
const SATURATED_PENALTY: Duration = Duration::from_secs(1);
const SHALLOW_PENALTY: Duration = Duration::from_secs(5);
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    EvmJsonRpc,
    TronHttp,
}

impl Dialect {
    pub fn node_url(self, url: Url) -> Url {
        match self {
            Self::EvmJsonRpc => url,
            Self::TronHttp => {
                let trimmed = url.as_str().trim_end_matches('/');
                let base = trimmed.strip_suffix("/jsonrpc").unwrap_or(trimmed);
                Url::parse(base).unwrap_or(url)
            }
        }
    }
}

pub struct RpcPoolConfig {
    pub chain_id: u64,
    pub dialect: Dialect,
    pub rps: u32,
    pub own: Vec<Url>,
    pub seeds: Vec<Url>,
    pub exclude: Vec<String>,
    pub max_public: usize,
}

struct Tried<'a> {
    method: &'a str,
    requests: usize,
    attempt: u32,
    latency: Duration,
}

enum Failure<E> {
    Retry(Retry),
    Fatal(E),
}

struct NodeState {
    limiter: RateLimiter,
    unsupported: Mutex<HashSet<String>>,
    shallow: AtomicBool,
}

struct Node {
    url: Url,
    http: Http<Client>,
    latency: Duration,
    state: Arc<NodeState>,
    personal: bool,
}

impl Node {
    fn host(&self) -> &str {
        self.url.host_str().unwrap_or("?")
    }

    fn label(&self) -> String {
        if self.personal {
            PERSONAL_LABEL.to_owned()
        } else {
            node_label(&self.url)
        }
    }

    fn supports(&self, method: &str) -> bool {
        !self
            .state
            .unsupported
            .lock()
            .expect("the lock is never poisoned")
            .contains(method)
    }
}

pub struct RpcPool {
    http: Client,
    config: RpcPoolConfig,
    chainlist: Option<Arc<Chainlist>>,
    nodes: RwLock<Arc<Vec<Arc<Node>>>>,
    personal: Mutex<HashMap<String, Arc<NodeState>>>,
    monitor: OnceLock<Arc<RpcMonitor>>,
}

#[derive(Clone, Debug)]
pub struct PoolNode {
    pub host: String,
    pub own: bool,
    pub probe_latency_ms: Option<u64>,
    pub shallow: bool,
    pub saturated: bool,
    pub unsupported: Vec<String>,
}

impl RpcPool {
    pub fn new(
        http: Client,
        config: RpcPoolConfig,
        chainlist: Option<Arc<Chainlist>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            http,
            config,
            chainlist,
            nodes: RwLock::new(Arc::new(Vec::new())),
            personal: Mutex::new(HashMap::new()),
            monitor: OnceLock::new(),
        })
    }

    pub fn watch(&self, monitor: Arc<RpcMonitor>) {
        let _ = self.monitor.set(monitor);
    }

    pub fn nodes(&self) -> Vec<PoolNode> {
        let own: HashSet<String> = self
            .config
            .own
            .iter()
            .map(|url| key(&self.config.dialect.node_url(url.clone())))
            .collect();

        self.snapshot()
            .iter()
            .map(|node| {
                let mut unsupported: Vec<String> = node
                    .state
                    .unsupported
                    .lock()
                    .expect("the lock is never poisoned")
                    .iter()
                    .cloned()
                    .collect();
                unsupported.sort();
                PoolNode {
                    host: node.label(),
                    own: own.contains(&key(&node.url)),
                    probe_latency_ms: (node.latency != Duration::MAX
                        && node.latency != Duration::ZERO)
                        .then(|| node.latency.as_millis() as u64),
                    shallow: node.state.shallow.load(Ordering::Relaxed),
                    saturated: node.state.limiter.saturated(),
                    unsupported,
                }
            })
            .collect()
    }

    fn observe(&self, node: &Node, tried: &Tried<'_>, outcome: CallOutcome, detail: Option<&str>) {
        let Some(monitor) = self.monitor.get() else {
            return;
        };
        let label = node.label();
        monitor.record(CallRecord {
            chain_id: self.config.chain_id,
            node: &label,
            method: tried.method,
            requests: tried.requests,
            attempt: tried.attempt,
            latency: tried.latency,
            outcome,
            detail: detail.map(|detail| redact(detail, &[node.url.as_str()], &label)),
        });
    }

    pub fn fixed(http: Client, urls: Vec<Url>, rps: u32) -> Arc<Self> {
        let pool = Self::new(
            http,
            RpcPoolConfig {
                chain_id: 0,
                dialect: Dialect::EvmJsonRpc,
                rps,
                own: urls.clone(),
                seeds: Vec::new(),
                exclude: Vec::new(),
                max_public: 0,
            },
            None,
        );

        let nodes = urls
            .into_iter()
            .map(|url| pool.node(url, Duration::ZERO, &HashMap::new()))
            .collect();
        pool.replace(nodes);

        pool
    }

    pub fn chain_id(&self) -> u64 {
        self.config.chain_id
    }

    pub fn dialect(&self) -> Dialect {
        self.config.dialect
    }

    pub fn len(&self) -> usize {
        self.snapshot().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn transport(self: &Arc<Self>) -> PoolTransport {
        PoolTransport { pool: self.clone() }
    }

    pub async fn rescan(&self, probes: &Semaphore) -> io::Result<()> {
        let chain_id = self.config.chain_id;
        let candidates = self.candidates();
        let total = candidates.len();
        let probed = probe::all(
            &self.http,
            candidates,
            chain_id,
            self.config.dialect,
            probes,
        )
        .await;
        let chosen = self.choose(probed);

        let existing: HashMap<String, Arc<NodeState>> = self
            .snapshot()
            .iter()
            .map(|node| (key(&node.url), node.state.clone()))
            .collect();

        let nodes: Vec<Arc<Node>> = if chosen.is_empty() {
            if self.config.own.is_empty() {
                return Err(io::Error::other(format!(
                    "none of {total} rpc nodes of chain {chain_id} answered the probe"
                )));
            }
            tracing::warn!(
                "no rpc node of chain {chain_id} passed the probe, falling back to the configured ones"
            );
            self.config
                .own
                .iter()
                .filter(|url| !self.excluded(url))
                .map(|url| self.node(url.clone(), Duration::MAX, &existing))
                .collect()
        } else {
            chosen
                .into_iter()
                .map(|probed| self.node(probed.url, probed.latency, &existing))
                .collect()
        };

        if nodes.is_empty() {
            return Err(io::Error::other(format!(
                "every rpc node of chain {chain_id} is excluded"
            )));
        }

        let summary: Vec<String> = nodes
            .iter()
            .take(3)
            .map(|node| format!("{} {}ms", node.host(), node.latency.as_millis()))
            .collect();
        tracing::info!(
            "chain {chain_id}: rpc pool holds {} of {total} nodes, fastest: {}",
            nodes.len(),
            summary.join(", ")
        );

        self.replace(nodes);
        Ok(())
    }

    fn candidates(&self) -> Vec<Candidate> {
        let mut seen = HashSet::new();
        let mut candidates = Vec::new();

        let dialect = self.config.dialect;

        for url in &self.config.own {
            let url = dialect.node_url(url.clone());
            if seen.insert(key(&url)) {
                candidates.push(Candidate { url, own: true });
            }
        }

        if self.config.max_public > 0 {
            let listed = self
                .chainlist
                .as_ref()
                .map(|chainlist| chainlist.rpc_urls(self.config.chain_id))
                .unwrap_or_default();

            for url in listed.into_iter().chain(self.config.seeds.iter().cloned()) {
                let url = dialect.node_url(url);
                if seen.insert(key(&url)) {
                    candidates.push(Candidate { url, own: false });
                }
            }
        }

        candidates.retain(|candidate| !self.excluded(&candidate.url));
        candidates
    }

    fn choose(&self, mut probed: Vec<Probed>) -> Vec<Probed> {
        let mut heads: Vec<u64> = probed.iter().map(|probed| probed.head).collect();
        heads.sort_unstable();
        let Some(&median) = heads.get(heads.len() / 2) else {
            return probed;
        };
        let spread = heads.last().map_or(0, |&highest| highest - median);
        let floor = median.saturating_sub(MAX_HEAD_LAG.max(spread * 2));

        probed.retain(|probed| {
            if probed.head < floor {
                tracing::debug!(
                    "rpc {} lags {} blocks behind, leaving it out",
                    probed.url.host_str().unwrap_or("?"),
                    median - probed.head
                );
            }
            probed.head >= floor
        });
        probed.sort_by_key(|probed| probed.latency);

        let mut public = 0;
        probed.retain(|probed| {
            if probed.own {
                return true;
            }
            public += 1;
            public <= self.config.max_public
        });

        probed
    }

    fn excluded(&self, url: &Url) -> bool {
        let url_key = key(url);
        let host = url.host_str();

        self.config.exclude.iter().any(|rule| {
            let rule = rule.trim().trim_end_matches('/');
            !rule.is_empty() && (url_key == rule || host == Some(rule))
        })
    }

    fn node(
        &self,
        url: Url,
        latency: Duration,
        existing: &HashMap<String, Arc<NodeState>>,
    ) -> Arc<Node> {
        let state = existing
            .get(&key(&url))
            .cloned()
            .unwrap_or_else(|| self.fresh_state());

        Arc::new(Node {
            http: Http::with_client(self.http.clone(), url.clone()),
            url,
            latency,
            state,
            personal: false,
        })
    }

    fn fresh_state(&self) -> Arc<NodeState> {
        Arc::new(NodeState {
            limiter: RateLimiter::new(self.config.rps),
            unsupported: Mutex::new(HashSet::new()),
            shallow: AtomicBool::new(false),
        })
    }

    fn personal_nodes(&self) -> Vec<Arc<Node>> {
        let urls = personal::current(self.config.chain_id);
        if urls.is_empty() {
            return Vec::new();
        }

        let mut states = self.personal.lock().expect("the lock is never poisoned");
        urls.into_iter()
            .map(|url| {
                let url = self.config.dialect.node_url(url);
                let state = states
                    .entry(key(&url))
                    .or_insert_with(|| self.fresh_state())
                    .clone();
                Arc::new(Node {
                    http: Http::with_client(self.http.clone(), url.clone()),
                    url,
                    latency: Duration::ZERO,
                    state,
                    personal: true,
                })
            })
            .collect()
    }

    fn snapshot(&self) -> Arc<Vec<Arc<Node>>> {
        self.nodes
            .read()
            .expect("the lock is never poisoned")
            .clone()
    }

    fn replace(&self, nodes: Vec<Arc<Node>>) {
        *self.nodes.write().expect("the lock is never poisoned") = Arc::new(nodes);
    }

    async fn pick(&self, method: &str, tried: &[Url]) -> Option<Arc<Node>> {
        let personal = self.personal_nodes();
        let nodes = self.snapshot();
        let supporting: Vec<&Arc<Node>> = personal
            .iter()
            .chain(nodes.iter())
            .filter(|node| node.supports(method))
            .collect();
        let fresh: Vec<&Arc<Node>> = supporting
            .iter()
            .copied()
            .filter(|node| !tried.contains(&node.url))
            .collect();
        let choices = if fresh.is_empty() { supporting } else { fresh };

        let now = Instant::now();
        let mut best: Option<(Duration, &Arc<Node>)> = None;

        for node in choices {
            let wait = node
                .state
                .limiter
                .ready_at()
                .await
                .saturating_duration_since(now);
            let mut penalty = Duration::ZERO;
            if node.state.limiter.saturated() {
                penalty += SATURATED_PENALTY;
            }
            if node.state.shallow.load(Ordering::Relaxed) {
                penalty += SHALLOW_PENALTY;
            }
            let score = wait.saturating_add(node.latency).saturating_add(penalty);

            if best.is_none_or(|(best, _)| score < best) {
                best = Some((score, node));
            }
        }

        best.map(|(_, node)| node.clone())
    }

    pub async fn post_json(
        &self,
        path: &str,
        body: &Value,
        useful: fn(&Value) -> bool,
    ) -> io::Result<Value> {
        let path = path.trim_start_matches('/');

        self.attempt(
            path,
            1,
            |node| async move {
                let url = format!("{}/{path}", node.url.as_str().trim_end_matches('/'));
                let answer = self
                    .http
                    .post(url)
                    .timeout(HTTP_TIMEOUT)
                    .json(body)
                    .send()
                    .await;

                match answer {
                    Ok(response) => {
                        let status = response.status();
                        let text = response.text().await.unwrap_or_default();
                        http_answer(status, &text, useful)
                    }
                    Err(error) => Err(Failure::Retry(http_unreachable(&error))),
                }
            },
            io::Error::other,
        )
        .await
    }

    async fn send(&self, request: RequestPacket) -> Result<ResponsePacket, TransportError> {
        let requests = request.len();
        let method = request
            .method_names()
            .next()
            .unwrap_or("rpc request")
            .to_owned();

        self.attempt(
            &method,
            requests,
            |node| {
                let request = request.clone();
                let method = method.clone();
                async move {
                    match node.http.clone().call(request).await {
                        Ok(response) => match failed_response(&response, requests, &method) {
                            Some(retry) => Err(Failure::Retry(retry)),
                            None => Ok(response),
                        },
                        Err(error) => match retryable(error) {
                            Ok(retry) => Err(Failure::Retry(retry)),
                            Err(error) => Err(Failure::Fatal(error)),
                        },
                    }
                }
            },
            |message| TransportErrorKind::custom_str(&message),
        )
        .await
    }

    async fn attempt<T, E, F, Fut>(
        &self,
        method: &str,
        requests: usize,
        call: F,
        give_up: impl Fn(String) -> E,
    ) -> Result<T, E>
    where
        E: std::fmt::Display,
        F: Fn(Arc<Node>) -> Fut,
        Fut: Future<Output = Result<T, Failure<E>>>,
    {
        let mut tried = Vec::new();
        let mut attempt = 1;

        loop {
            let Some(node) = self.pick(method, &tried).await else {
                return Err(give_up(format!(
                    "no rpc node of chain {} serves {method}",
                    self.config.chain_id
                )));
            };

            let permit = node.state.limiter.acquire(requests).await;
            let started = Instant::now();
            let answer = call(node.clone()).await;
            let shot = Tried {
                method,
                requests,
                attempt,
                latency: started.elapsed(),
            };
            drop(permit);

            let retry = match answer {
                Ok(answer) => {
                    self.observe(&node, &shot, CallOutcome::Ok, None);
                    return Ok(answer);
                }
                Err(Failure::Fatal(error)) => {
                    let detail = error.to_string();
                    self.observe(&node, &shot, CallOutcome::Failed, Some(&detail));
                    return Err(error);
                }
                Err(Failure::Retry(retry)) => retry,
            };

            let outcome = if attempt >= MAX_ATTEMPTS {
                CallOutcome::GaveUp
            } else {
                match retry.kind {
                    RetryKind::Busy => CallOutcome::Busy,
                    RetryKind::Unsupported => CallOutcome::Unsupported,
                    RetryKind::Shallow => CallOutcome::Shallow,
                }
            };
            self.observe(&node, &shot, outcome, Some(&retry.reason));

            if attempt >= MAX_ATTEMPTS {
                tracing::warn!(
                    "chain {}: {method} gave up after {attempt} attempts, the last on {}: {}",
                    self.config.chain_id,
                    node.host(),
                    retry.reason
                );
                return Err(give_up(format!(
                    "{method} gave up after {attempt} attempts: {}",
                    retry.reason
                )));
            }

            self.cool_down(&node, method, &retry, attempt).await;
            tried.push(node.url.clone());
            attempt += 1;
        }
    }

    async fn cool_down(&self, node: &Node, method: &str, retry: &Retry, attempt: u32) {
        match retry.kind {
            RetryKind::Unsupported => {
                tracing::info!(
                    "rpc {} does not serve {method}: {}",
                    node.host(),
                    retry.reason
                );
                node.state
                    .unsupported
                    .lock()
                    .expect("the lock is never poisoned")
                    .insert(method.to_owned());
            }
            RetryKind::Shallow => {
                if !node.state.shallow.swap(true, Ordering::Relaxed) {
                    tracing::info!(
                        "rpc {} keeps a short history, asking it last from now on: {}",
                        node.host(),
                        retry.reason
                    );
                }
            }
            RetryKind::Busy => {
                let delay = retry.after.unwrap_or_else(|| backoff(attempt));
                tracing::debug!(
                    "{method} attempt {attempt}/{MAX_ATTEMPTS} on {} failed ({}), resting it for {delay:?}",
                    node.host(),
                    retry.reason
                );
                node.state.limiter.throttle(delay).await;
            }
        }
    }
}

const SHOWN_SEGMENT: usize = 12;

pub fn node_label(url: &Url) -> String {
    let host = url.host_str().unwrap_or("?");
    let path: Vec<String> = url
        .path_segments()
        .into_iter()
        .flatten()
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            if segment.chars().count() <= SHOWN_SEGMENT {
                segment.to_owned()
            } else {
                let tail: String = segment
                    .chars()
                    .rev()
                    .take(4)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                format!("…{tail}")
            }
        })
        .collect();
    match url.port() {
        Some(port) if path.is_empty() => format!("{host}:{port}"),
        Some(port) => format!("{host}:{port}/{}", path.join("/")),
        None if path.is_empty() => host.to_owned(),
        None => format!("{host}/{}", path.join("/")),
    }
}

fn key(url: &Url) -> String {
    url.as_str().trim_end_matches('/').to_owned()
}

#[derive(Clone)]
pub struct PoolTransport {
    pool: Arc<RpcPool>,
}

impl Service<RequestPacket> for PoolTransport {
    type Response = ResponsePacket;
    type Error = TransportError;
    type Future = TransportFut<'static>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: RequestPacket) -> Self::Future {
        let pool = self.pool.clone();
        Box::pin(async move { pool.send(request).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(exclude: &[&str], max_public: usize) -> Arc<RpcPool> {
        RpcPool::new(
            Client::new(),
            RpcPoolConfig {
                chain_id: 1,
                dialect: Dialect::EvmJsonRpc,
                rps: 10,
                own: Vec::new(),
                seeds: Vec::new(),
                exclude: exclude.iter().map(|rule| (*rule).to_owned()).collect(),
                max_public,
            },
            None,
        )
    }

    fn probed(url: &str, own: bool, latency_ms: u64, head: u64) -> Probed {
        Probed {
            url: Url::parse(url).unwrap(),
            own,
            latency: Duration::from_millis(latency_ms),
            head,
        }
    }

    #[test]
    fn a_node_label_tells_paths_apart_but_hides_keys() {
        let label = |raw: &str| node_label(&Url::parse(raw).unwrap());

        assert_eq!(
            label("https://rpc.mevblocker.io/fast"),
            "rpc.mevblocker.io/fast"
        );
        assert_eq!(label("https://rpc.mevblocker.io/"), "rpc.mevblocker.io");
        assert_eq!(
            label("https://mainnet.infura.io/v3/0123456789abcdef0123456789abcdef"),
            "mainnet.infura.io/v3/…cdef"
        );
        assert_eq!(label("http://127.0.0.1:8545"), "127.0.0.1:8545");
    }

    #[test]
    fn excludes_by_url_or_host() {
        let pool = pool(&["https://a.example/rpc/", "b.example"], 4);

        assert!(pool.excluded(&Url::parse("https://a.example/rpc").unwrap()));
        assert!(pool.excluded(&Url::parse("https://b.example/anything").unwrap()));
        assert!(!pool.excluded(&Url::parse("https://a.example/other").unwrap()));
    }

    #[test]
    fn keeps_the_fastest_public_nodes_and_every_own_one() {
        let chosen = pool(&[], 2).choose(vec![
            probed("https://slow.example", false, 300, 100),
            probed("https://mine.example", true, 900, 100),
            probed("https://fast.example", false, 50, 100),
            probed("https://mid.example", false, 120, 100),
            probed("https://stale.example", false, 10, 90),
            probed("https://jittery.example", false, 400, 98),
        ]);

        let hosts: Vec<_> = chosen
            .iter()
            .map(|probed| probed.url.host_str().unwrap())
            .collect();
        assert_eq!(hosts, ["fast.example", "mid.example", "mine.example"]);
    }

    #[tokio::test]
    async fn a_personal_node_goes_first_only_for_its_owner() {
        let pool = RpcPool::fixed(
            Client::new(),
            vec![Url::parse("https://shared.example").unwrap()],
            10,
        );
        let mine = || {
            personal::PersonalNodes::new(std::collections::BTreeMap::from([(
                0,
                vec!["https://mine.example/key".to_owned()],
            )]))
        };

        let picked = mine()
            .scope(pool.pick("eth_blockNumber", &[]))
            .await
            .unwrap();
        assert!(picked.personal);
        assert_eq!(picked.label(), PERSONAL_LABEL);

        let fallback = mine()
            .scope(pool.pick("eth_blockNumber", std::slice::from_ref(&picked.url)))
            .await
            .unwrap();
        assert_eq!(fallback.host(), "shared.example");

        let anonymous = pool.pick("eth_blockNumber", &[]).await.unwrap();
        assert_eq!(anonymous.host(), "shared.example");
    }
}
