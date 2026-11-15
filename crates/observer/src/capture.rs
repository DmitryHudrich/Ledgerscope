use std::{
    net::IpAddr,
    time::{SystemTime, UNIX_EPOCH},
};

use deeptls::{Classifier, ContentType, Packet, parse_records};

use crate::hosts::HostMatcher;

pub const MIN_APP_RECORDS: usize = 2;

pub struct Shared {
    pub classifier: Classifier,
    pub matcher: HostMatcher,
    pub http: reqwest::Client,
    pub ingest_url: String,
    pub token: String,
    pub default_port: u16,
    pub min_confidence: f64,
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

#[cfg_attr(not(feature = "pcap"), allow(dead_code))]
pub fn app_records(packets: &[Packet]) -> Option<usize> {
    let records = parse_records(packets).ok()?;
    Some(
        records
            .iter()
            .filter(|record| record.kind == ContentType::ApplicationData)
            .count(),
    )
}

pub async fn report(
    shared: &Shared,
    ip: IpAddr,
    chain_id: u64,
    host: &str,
    observed_at: u64,
    packets: &[Packet],
) {
    let records = match parse_records(packets) {
        Ok(records) => records,
        Err(error) => {
            tracing::debug!("{ip} -> {host}: not parseable tls ({error:?})");
            return;
        }
    };

    let count = records
        .iter()
        .filter(|record| record.kind == ContentType::ApplicationData)
        .count();
    if count < MIN_APP_RECORDS {
        return;
    }

    let verdict = shared.classifier.classify(&records);
    if verdict.confidence < shared.min_confidence {
        return;
    }

    let body = serde_json::json!({
        "src_ip": ip.to_string(),
        "chain_id": chain_id,
        "intent": verdict.intent.as_str(),
        "confidence": verdict.confidence,
        "observed_at": observed_at,
    });

    match shared
        .http
        .post(&shared.ingest_url)
        .bearer_auth(&shared.token)
        .json(&body)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => tracing::info!(
            "{ip} -> {host} [chain {chain_id}]: {} ({:.2})",
            verdict.intent.as_str(),
            verdict.confidence
        ),
        Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::warn!(
                "{ip} -> {host}: ingest rejected with {status}: {}",
                body.trim()
            );
        }
        Err(error) => tracing::warn!("{ip} -> {host}: ingest failed: {error}"),
    }
}

pub fn make_packet(time: f64, up: bool, payload: Vec<u8>) -> Packet {
    Packet {
        time,
        dir: if up {
            deeptls::Direction::Up
        } else {
            deeptls::Direction::Down
        },
        payload,
    }
}
