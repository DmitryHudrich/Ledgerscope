use serde::Serialize;
use utoipa::ToSchema;

use adapters::eth::{NodeStats, PoolNode, RpcCall};
use application::eth::{BuildJob, ScanJob};

use crate::dto::RangeResponse;

#[derive(Serialize, ToSchema)]
pub struct CallResponse {
    id: u64,
    at_ms: u64,
    chain_id: u64,
    node: String,
    method: String,
    requests: usize,
    attempt: u32,
    latency_ms: u64,
    outcome: &'static str,
    detail: Option<String>,
}

impl From<&RpcCall> for CallResponse {
    fn from(call: &RpcCall) -> Self {
        Self {
            id: call.id,
            at_ms: call.at_ms,
            chain_id: call.chain_id,
            node: call.node.clone(),
            method: call.method.clone(),
            requests: call.requests,
            attempt: call.attempt,
            latency_ms: call.latency_ms,
            outcome: call.outcome.as_str(),
            detail: call.detail.clone(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct NodeErrorResponse {
    at_ms: u64,
    outcome: &'static str,
    detail: String,
}

#[derive(Serialize, ToSchema)]
pub struct NodeHealthResponse {
    chain_id: u64,
    host: String,
    in_pool: bool,
    own: bool,
    probe_latency_ms: Option<u64>,
    shallow: bool,
    saturated: bool,
    unsupported: Vec<String>,
    calls: u64,
    requests: u64,
    failures: u64,
    recent_error_rate: f64,
    average_latency_ms: Option<u64>,
    p50_latency_ms: Option<u64>,
    p95_latency_ms: Option<u64>,
    last_ok_ms: Option<u64>,
    last_error: Option<NodeErrorResponse>,
}

impl NodeHealthResponse {
    pub fn new(chain_id: u64, pool: Option<&PoolNode>, stats: Option<&NodeStats>) -> Self {
        Self {
            chain_id,
            host: pool
                .map(|node| node.host.clone())
                .or_else(|| stats.map(|stats| stats.node.clone()))
                .unwrap_or_default(),
            in_pool: pool.is_some(),
            own: pool.is_some_and(|node| node.own),
            probe_latency_ms: pool.and_then(|node| node.probe_latency_ms),
            shallow: pool.is_some_and(|node| node.shallow),
            saturated: pool.is_some_and(|node| node.saturated),
            unsupported: pool
                .map(|node| node.unsupported.clone())
                .unwrap_or_default(),
            calls: stats.map_or(0, |stats| stats.calls),
            requests: stats.map_or(0, |stats| stats.requests),
            failures: stats.map_or(0, |stats| stats.failures),
            recent_error_rate: stats.map_or(0.0, NodeStats::recent_error_rate),
            average_latency_ms: stats.and_then(NodeStats::average_latency_ms),
            p50_latency_ms: stats.and_then(|stats| stats.recent_latency_ms(0.5)),
            p95_latency_ms: stats.and_then(|stats| stats.recent_latency_ms(0.95)),
            last_ok_ms: stats.and_then(|stats| stats.last_ok_ms),
            last_error: stats.and_then(|stats| {
                stats
                    .last_error
                    .as_ref()
                    .map(|(at_ms, outcome, detail)| NodeErrorResponse {
                        at_ms: *at_ms,
                        outcome: outcome.as_str(),
                        detail: detail.clone(),
                    })
            }),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct BuildResponse {
    id: u64,
    chain_id: u64,
    stage: &'static str,
    done: u64,
    total: u64,
    percent: f64,
    txs: u64,
    started_ms: u64,
    updated_ms: u64,
    finished_ms: Option<u64>,
    running: bool,
    error: Option<String>,
}

impl From<&BuildJob> for BuildResponse {
    fn from(job: &BuildJob) -> Self {
        Self {
            id: job.id,
            chain_id: job.chain_id,
            stage: job.stage.as_str(),
            done: job.done,
            total: job.total,
            percent: job.percent(),
            txs: job.txs,
            started_ms: job.started_ms,
            updated_ms: job.updated_ms,
            finished_ms: job.finished_ms,
            running: job.running(),
            error: job.error.clone(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ScanResponse {
    id: u64,
    chain_id: u64,
    ranges: Vec<RangeResponse>,
    total_blocks: u64,
    done_blocks: u64,
    percent: f64,
    txs: u64,
    started_ms: u64,
    updated_ms: u64,
    finished_ms: Option<u64>,
    running: bool,
    blocks_per_second: Option<f64>,
    eta_ms: Option<u64>,
    error: Option<String>,
}

impl From<&ScanJob> for ScanResponse {
    fn from(job: &ScanJob) -> Self {
        Self {
            id: job.id,
            chain_id: job.chain_id,
            ranges: job
                .ranges
                .iter()
                .copied()
                .map(RangeResponse::from)
                .collect(),
            total_blocks: job.total_blocks,
            done_blocks: job.done_blocks,
            percent: if job.total_blocks == 0 {
                100.0
            } else {
                job.done_blocks as f64 * 100.0 / job.total_blocks as f64
            },
            txs: job.txs,
            started_ms: job.started_ms,
            updated_ms: job.updated_ms,
            finished_ms: job.finished_ms,
            running: job.running(),
            blocks_per_second: job.blocks_per_second(),
            eta_ms: job.eta_ms(),
            error: job.error.clone(),
        }
    }
}
