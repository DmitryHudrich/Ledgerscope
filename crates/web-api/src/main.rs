mod dto;
mod routes;
mod state;

use anyhow::Context;

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:3000";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let state = state::AppState::from_env()?;
    let app = routes::router(state);

    // Containers have to bind 0.0.0.0 to be reachable; local runs stay on loopback.
    let bind_addr =
        std::env::var("BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_owned());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("failed to bind {bind_addr}"))?;
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await?;
    Ok(())
}
