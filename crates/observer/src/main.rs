mod capture;
mod config;
mod hosts;
mod proxy;
mod sni;
#[cfg(feature = "pcap")]
mod sniff;

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, bail};
use clap::Parser;
use deeptls::{Classifier, TrainConfig};
use tracing_subscriber::EnvFilter;

use crate::{
    capture::Shared,
    config::{Config, Mode},
    hosts::HostMatcher,
    proxy::Front,
};

#[derive(Parser)]
#[command(about = "Passive tls traffic observer: sni passthrough, http/socks proxy or pcap sniff")]
struct Cli {
    #[arg(long, default_value = "observer.yaml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let cli = Cli::parse();
    let raw = std::fs::read_to_string(&cli.config)
        .with_context(|| format!("cannot read config {}", cli.config.display()))?;
    let config: Config = serde_yaml::from_str(&raw)
        .with_context(|| format!("cannot parse config {}", cli.config.display()))?;

    for group in &config.groups {
        tracing::info!(
            "group {} -> chain {} ({} host patterns)",
            group.family,
            group.chain_id,
            group.hosts.len()
        );
    }

    let shared = Arc::new(Shared {
        classifier: Classifier::train(TrainConfig::default()),
        matcher: HostMatcher::build(&config.groups),
        http: reqwest::Client::new(),
        ingest_url: format!("{}/tls/observations", config.api.trim_end_matches('/')),
        token: config.token.clone(),
        default_port: config.default_port,
        min_confidence: config.min_confidence,
    });

    match config.mode {
        Mode::Sni | Mode::Proxy => {
            if config.listen.trim().is_empty() {
                bail!("listen must be set for the {:?} mode", config.mode);
            }
            let front = if config.mode == Mode::Proxy {
                Front::Proxy
            } else {
                Front::Sni
            };
            proxy::serve(shared, &config.listen, front).await?;
        }
        Mode::Sniff => {
            #[cfg(feature = "pcap")]
            sniff::run(shared, &config.sniff).await?;
            #[cfg(not(feature = "pcap"))]
            bail!(
                "built without the pcap feature; rebuild with: cargo build -p ledgerscope-observer --features pcap"
            );
        }
    }
    Ok(())
}
