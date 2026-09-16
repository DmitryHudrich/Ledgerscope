mod config;
mod dto;
mod routes;
mod state;

use anyhow::Context;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = Config::load()?;

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
