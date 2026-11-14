use std::time::{Duration, Instant};

use futures::StreamExt;
use reqwest::{Client, Url};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use super::Dialect;
use crate::eth::tron;

const PROBE_TIMEOUT: Duration = Duration::from_secs(4);
const PROBES_IN_FLIGHT: usize = 32;

pub struct Candidate {
    pub url: Url,
    pub own: bool,
}

pub struct Probed {
    pub url: Url,
    pub own: bool,
    pub latency: Duration,
    pub head: u64,
}

pub async fn all(
    http: &Client,
    candidates: Vec<Candidate>,
    chain_id: u64,
    dialect: Dialect,
    probes: &Semaphore,
) -> Vec<Probed> {
    futures::stream::iter(candidates)
        .map(|candidate| async move {
            let _permit = probes
                .acquire()
                .await
                .expect("the semaphore is never closed");
            match one(http, &candidate.url, chain_id, dialect).await {
                Ok((latency, head)) => Some(Probed {
                    url: candidate.url,
                    own: candidate.own,
                    latency,
                    head,
                }),
                Err(reason) => {
                    let host = candidate.url.host_str().unwrap_or("?");
                    if candidate.own {
                        tracing::warn!("rpc {host} did not pass the probe: {reason}");
                    } else {
                        tracing::debug!("rpc {host} did not pass the probe: {reason}");
                    }
                    None
                }
            }
        })
        .buffer_unordered(PROBES_IN_FLIGHT)
        .filter_map(std::future::ready)
        .collect()
        .await
}

pub async fn one(
    http: &Client,
    url: &Url,
    chain_id: u64,
    dialect: Dialect,
) -> Result<(Duration, u64), String> {
    match dialect {
        Dialect::EvmJsonRpc => evm(http, url, chain_id).await,
        Dialect::TronHttp => tron_node(http, url, chain_id).await,
    }
}

pub async fn served_chain(http: &Client, url: &Url, dialect: Dialect) -> Result<u64, String> {
    match dialect {
        Dialect::EvmJsonRpc => {
            let answer = post(
                http,
                url.as_str(),
                json!({"jsonrpc": "2.0", "id": 1, "method": "eth_chainId", "params": []}),
            )
            .await?;
            answer["result"]
                .as_str()
                .and_then(|hex| u64::from_str_radix(hex.trim_start_matches("0x"), 16).ok())
                .ok_or_else(|| "no eth_chainId in the answer".to_owned())
        }
        Dialect::TronHttp => tron_chain(http, url.as_str().trim_end_matches('/')).await,
    }
}

async fn tron_chain(http: &Client, base: &str) -> Result<u64, String> {
    let genesis = post(
        http,
        &format!("{base}/wallet/getblockbynum"),
        json!({ "num": 0 }),
    )
    .await?;
    genesis["blockID"]
        .as_str()
        .and_then(tron::chain_id_of_genesis)
        .ok_or_else(|| "no genesis block id in getblockbynum".to_owned())
}

async fn evm(http: &Client, url: &Url, chain_id: u64) -> Result<(Duration, u64), String> {
    let body = json!([
        {"jsonrpc": "2.0", "id": 1, "method": "eth_chainId", "params": []},
        {"jsonrpc": "2.0", "id": 2, "method": "eth_blockNumber", "params": []},
    ]);

    let started = Instant::now();
    let answers: Vec<Value> = http
        .post(url.clone())
        .timeout(PROBE_TIMEOUT)
        .json(&body)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| format!("no batch support: {error}"))?;
    let latency = started.elapsed();

    let quantity = |id: u64| {
        answers
            .iter()
            .find(|answer| answer["id"].as_u64() == Some(id))
            .and_then(|answer| answer["result"].as_str())
            .and_then(|hex| u64::from_str_radix(hex.trim_start_matches("0x"), 16).ok())
    };

    let served = quantity(1).ok_or("no eth_chainId in the answer")?;
    if served != chain_id {
        return Err(format!("serves chain {served}, not {chain_id}"));
    }

    let head = quantity(2).ok_or("no eth_blockNumber in the answer")?;

    Ok((latency, head))
}

async fn tron_node(http: &Client, url: &Url, chain_id: u64) -> Result<(Duration, u64), String> {
    let base = url.as_str().trim_end_matches('/');

    let served = tron_chain(http, base).await?;
    if served != chain_id {
        return Err(format!("serves chain {served}, not {chain_id}"));
    }

    let started = Instant::now();
    let now = post(
        http,
        &format!("{base}/wallet/getblock"),
        json!({ "detail": false }),
    )
    .await?;
    let latency = started.elapsed();

    let head = now["block_header"]["raw_data"]["number"]
        .as_u64()
        .ok_or("no block number in getblock")?;

    Ok((latency, head))
}

async fn post(http: &Client, url: &str, body: Value) -> Result<Value, String> {
    http.post(url)
        .timeout(PROBE_TIMEOUT)
        .json(&body)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| format!("not a node: {error}"))
}
