use axum::{
    Json, Router,
    extract::{Query, State},
    response::Html,
    routing::get,
};
use serde::Deserialize;

use crate::{dto::GraphResponse, state::AppState};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/graph", get(graph))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}

#[derive(Deserialize)]
pub struct GraphQuery {
    #[allow(dead_code)]
    wallet: String,
    from: u64,
    to: u64,
}

impl GraphQuery {
    #[allow(clippy::wrong_self_convention)]
    pub fn from_block(&self) -> u64 {
        self.from
    }

    pub fn to_block(&self) -> u64 {
        self.to
    }
}

async fn graph(
    State(state): State<AppState>,
    Query(query): Query<GraphQuery>,
) -> Json<GraphResponse> {
    state
        .eth_fetcher()
        .seed_eth(query.from_block(), query.to_block())
        .await;
    Json(state.eth_fetcher().full_graph().await.into())
}
