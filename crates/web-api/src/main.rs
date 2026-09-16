mod config;
mod dto;
mod routes;
mod state;

use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

#[derive(Debug, Parser)]
#[command(about = "Ledgerscope web API")]
struct Cli {
    #[arg(
        short,
        long,
        value_name = "PATH",
        help = "Path to the YAML config. Defaults to ./config.yaml when it exists."
    )]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();
    let config = Config::load(cli.config.as_deref())?;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(&config.log.filter).with_context(|| {
                format!("log.filter is not a valid filter: {}", config.log.filter)
            })?,
        )
        .init();

    let state = state::AppState::from_config(&config)?;
    let app = routes::router(state);

    let listener = tokio::net::TcpListener::bind(&config.server.bind_addr)
        .await
        .with_context(|| format!("failed to bind {}", config.server.bind_addr))?;
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await?;
    Ok(())
}
