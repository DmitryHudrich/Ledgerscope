use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use tokio::sync::broadcast;

const RECENT_CALLS: usize = 2_000;
const NODE_WINDOW: usize = 200;
const STREAM_BUFFER: usize = 1_024;
const DETAIL_LIMIT: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallOutcome {
    Ok,
    Busy,
    Unsupported,
    Shallow,
    Failed,
    GaveUp,
}

impl CallOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Busy => "busy",
            Self::Unsupported => "unsupported",
            Self::Shallow => "shallow",
            Self::Failed => "failed",
            Self::GaveUp => "gave_up",
        }
    }

    pub fn ok(self) -> bool {
        self == Self::Ok
    }
}

#[derive(Clone, Debug)]
pub struct RpcCall {
    pub id: u64,
    pub at_ms: u64,
    pub chain_id: u64,
    pub node: String,
    pub method: String,
    pub requests: usize,
    pub attempt: u32,
    pub latency_ms: u64,
    pub outcome: CallOutcome,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct NodeStats {
    pub chain_id: u64,
    pub node: String,
    pub calls: u64,
    pub requests: u64,
    pub failures: u64,
    pub latency_sum_ms: u64,
    pub last_ok_ms: Option<u64>,
    pub last_error: Option<(u64, CallOutcome, String)>,
    window: VecDeque<(bool, u64)>,
}

impl NodeStats {
    pub fn average_latency_ms(&self) -> Option<u64> {
        let ok = self.calls.saturating_sub(self.failures);
        (ok > 0).then(|| self.latency_sum_ms / ok)
    }

    pub fn recent_error_rate(&self) -> f64 {
        if self.window.is_empty() {
            return 0.0;
        }
        let failed = self.window.iter().filter(|(ok, _)| !ok).count();
        failed as f64 / self.window.len() as f64
    }

    pub fn recent_latency_ms(&self, quantile: f64) -> Option<u64> {
        let mut latencies: Vec<u64> = self
            .window
            .iter()
            .filter(|(ok, _)| *ok)
            .map(|(_, latency)| *latency)
            .collect();
        if latencies.is_empty() {
            return None;
        }
        latencies.sort_unstable();
        let index = ((latencies.len() - 1) as f64 * quantile).round() as usize;
        latencies.get(index).copied()
    }

    fn record(&mut self, call: &RpcCall) {
        self.calls += 1;
        self.requests += call.requests as u64;
        if call.outcome.ok() {
            self.latency_sum_ms += call.latency_ms;
            self.last_ok_ms = Some(call.at_ms);
        } else {
            self.failures += 1;
            self.last_error = Some((
                call.at_ms,
                call.outcome,
                call.detail.clone().unwrap_or_default(),
            ));
        }
        self.window.push_back((call.outcome.ok(), call.latency_ms));
        if self.window.len() > NODE_WINDOW {
            self.window.pop_front();
        }
    }
}

pub struct RpcMonitor {
    next_id: AtomicU64,
    recent: Mutex<VecDeque<RpcCall>>,
    nodes: Mutex<HashMap<(u64, String), NodeStats>>,
    stream: broadcast::Sender<RpcCall>,
}

impl Default for RpcMonitor {
    fn default() -> Self {
        let (stream, _) = broadcast::channel(STREAM_BUFFER);
        Self {
            next_id: AtomicU64::new(1),
            recent: Mutex::new(VecDeque::with_capacity(RECENT_CALLS)),
            nodes: Mutex::new(HashMap::new()),
            stream,
        }
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

pub fn redact(detail: &str, secrets: &[&str], host: &str) -> String {
    let mut clean = detail.to_owned();
    for secret in secrets {
        let secret = secret.trim_end_matches('/');
        if !secret.is_empty() {
            clean = clean.replace(secret, host);
        }
    }
    if clean.chars().count() > DETAIL_LIMIT {
        clean = clean.chars().take(DETAIL_LIMIT).collect::<String>() + "…";
    }
    clean
}

pub struct CallRecord<'a> {
    pub chain_id: u64,
    pub node: &'a str,
    pub method: &'a str,
    pub requests: usize,
    pub attempt: u32,
    pub latency: Duration,
    pub outcome: CallOutcome,
    pub detail: Option<String>,
}

impl RpcMonitor {
    pub fn record(&self, record: CallRecord<'_>) {
        let call = RpcCall {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            at_ms: now_ms(),
            chain_id: record.chain_id,
            node: record.node.to_owned(),
            method: record.method.to_owned(),
            requests: record.requests,
            attempt: record.attempt,
            latency_ms: record.latency.as_millis() as u64,
            outcome: record.outcome,
            detail: record.detail,
        };

        {
            let mut nodes = self.nodes.lock().expect("the lock is never poisoned");
            let stats = nodes
                .entry((call.chain_id, call.node.clone()))
                .or_insert_with(|| NodeStats {
                    chain_id: call.chain_id,
                    node: call.node.clone(),
                    ..NodeStats::default()
                });
            stats.record(&call);
        }

        {
            let mut recent = self.recent.lock().expect("the lock is never poisoned");
            if recent.len() == RECENT_CALLS {
                recent.pop_front();
            }
            recent.push_back(call.clone());
        }

        let _ = self.stream.send(call);
    }

    pub fn recent(&self, chain_id: Option<u64>, limit: usize) -> Vec<RpcCall> {
        let recent = self.recent.lock().expect("the lock is never poisoned");
        let mut picked: Vec<RpcCall> = recent
            .iter()
            .rev()
            .filter(|call| chain_id.is_none_or(|chain_id| call.chain_id == chain_id))
            .take(limit)
            .cloned()
            .collect();
        picked.reverse();
        picked
    }

    pub fn node_stats(&self, chain_id: Option<u64>) -> Vec<NodeStats> {
        let nodes = self.nodes.lock().expect("the lock is never poisoned");
        let mut stats: Vec<NodeStats> = nodes
            .values()
            .filter(|stats| chain_id.is_none_or(|chain_id| stats.chain_id == chain_id))
            .cloned()
            .collect();
        stats.sort_by(|a, b| a.chain_id.cmp(&b.chain_id).then(b.calls.cmp(&a.calls)));
        stats
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RpcCall> {
        self.stream.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(monitor: &RpcMonitor, chain_id: u64, node: &str, outcome: CallOutcome, latency: u64) {
        monitor.record(CallRecord {
            chain_id,
            node,
            method: "eth_getBlockByNumber",
            requests: 3,
            attempt: 1,
            latency: Duration::from_millis(latency),
            outcome,
            detail: (!outcome.ok()).then(|| "429 Too Many Requests".to_owned()),
        });
    }

    #[test]
    fn calls_are_kept_in_order_and_filtered_by_chain() {
        let monitor = RpcMonitor::default();
        call(&monitor, 1, "a", CallOutcome::Ok, 10);
        call(&monitor, 10, "b", CallOutcome::Ok, 20);
        call(&monitor, 1, "a", CallOutcome::Busy, 30);

        let ethereum = monitor.recent(Some(1), 10);
        assert_eq!(ethereum.len(), 2);
        assert!(ethereum[0].id < ethereum[1].id);
        assert_eq!(monitor.recent(None, 1)[0].outcome, CallOutcome::Busy);
    }

    #[test]
    fn node_stats_count_failures_and_latency() {
        let monitor = RpcMonitor::default();
        call(&monitor, 1, "a", CallOutcome::Ok, 100);
        call(&monitor, 1, "a", CallOutcome::Ok, 300);
        call(&monitor, 1, "a", CallOutcome::Busy, 5);
        call(&monitor, 1, "a", CallOutcome::Ok, 200);

        let stats = &monitor.node_stats(Some(1))[0];
        assert_eq!(stats.calls, 4);
        assert_eq!(stats.requests, 12);
        assert_eq!(stats.failures, 1);
        assert_eq!(stats.average_latency_ms(), Some(200));
        assert_eq!(stats.recent_latency_ms(0.5), Some(200));
        assert!((stats.recent_error_rate() - 0.25).abs() < f64::EPSILON);
        assert_eq!(stats.last_error.as_ref().unwrap().1, CallOutcome::Busy);
    }

    #[test]
    fn the_ring_forgets_the_oldest_calls() {
        let monitor = RpcMonitor::default();
        for _ in 0..RECENT_CALLS + 5 {
            call(&monitor, 1, "a", CallOutcome::Ok, 1);
        }
        let all = monitor.recent(None, usize::MAX);
        assert_eq!(all.len(), RECENT_CALLS);
        assert_eq!(all[0].id, 6);
    }

    #[test]
    fn a_key_in_the_url_never_reaches_the_log() {
        let clean = redact(
            "error sending request for url (https://mainnet.infura.io/v3/SECRET)",
            &["https://mainnet.infura.io/v3/SECRET"],
            "mainnet.infura.io",
        );
        assert_eq!(clean, "error sending request for url (mainnet.infura.io)");
    }

    #[tokio::test]
    async fn subscribers_hear_every_call() {
        let monitor = RpcMonitor::default();
        let mut listener = monitor.subscribe();
        call(&monitor, 1, "a", CallOutcome::Ok, 1);
        assert_eq!(listener.recv().await.unwrap().node, "a");
    }
}
