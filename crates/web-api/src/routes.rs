use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use application::eth::{Exploration, ExploreError, ExploreRequest, GraphRoot};
use domain::eth::BlockRange;

use crate::{
    dto::{
        CoverageResponse, GraphRequest, GraphResponse, HistogramResponse, RpcConfirmationResponse,
        parse_address,
    },
    state::AppState,
};

const DEFAULT_BUCKETS: u32 = 120;
const MAX_BUCKETS: u32 = 1_000;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/graph", post(graph))
        .route("/coverage", get(coverage))
        .route("/coverage/histogram", get(histogram))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn upstream(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}

impl From<ExploreError> for ApiError {
    fn from(error: ExploreError) -> Self {
        match error {
            ExploreError::Io(error) => Self::upstream(error.to_string()),
            other => Self::bad_request(other.to_string()),
        }
    }
}

async fn graph(
    State(state): State<AppState>,
    Json(request): Json<GraphRequest>,
) -> Result<Response, ApiError> {
    if request.from_block > request.to_block {
        return Err(ApiError::bad_request(
            "from_block must not be above to_block",
        ));
    }

    let mut roots = Vec::with_capacity(request.roots.len());
    for root in &request.roots {
        roots.push(GraphRoot::new(
            parse_address(&root.address).map_err(ApiError::bad_request)?,
            root.depth,
        ));
    }

    let span = BlockRange::new(request.from_block, request.to_block);
    let asked = ExploreRequest::new(roots, span, request.confirm_rpc);

    match state.explorer().explore(asked).await? {
        Exploration::Graph(graph) => Ok(Json(GraphResponse::new(&graph, span)).into_response()),
        Exploration::RpcNeeded(plan) => Ok((
            StatusCode::CONFLICT,
            Json(RpcConfirmationResponse::new(&plan, span)),
        )
            .into_response()),
    }
}

#[derive(Deserialize)]
pub struct SpanQuery {
    from_block: Option<u64>,
    to_block: Option<u64>,
}

impl SpanQuery {
    fn span(&self) -> Result<Option<BlockRange>, ApiError> {
        span_of(self.from_block, self.to_block)
    }
}

fn span_of(from_block: Option<u64>, to_block: Option<u64>) -> Result<Option<BlockRange>, ApiError> {
    match (from_block, to_block) {
        (None, None) => Ok(None),
        (Some(from), Some(to)) if from <= to => Ok(Some(BlockRange::new(from, to))),
        (Some(_), Some(_)) => Err(ApiError::bad_request(
            "from_block must not be above to_block",
        )),
        _ => Err(ApiError::bad_request("from_block and to_block go together")),
    }
}

async fn coverage(
    State(state): State<AppState>,
    Query(query): Query<SpanQuery>,
) -> Result<Json<CoverageResponse>, ApiError> {
    let span = query.span()?.unwrap_or(BlockRange::new(0, u64::MAX));
    let coverage = state
        .explorer()
        .coverage(span)
        .await
        .map_err(|error| ApiError::upstream(error.to_string()))?;

    Ok(Json(CoverageResponse::new(
        &coverage,
        state.explorer().persistent(),
        state.head_block().await,
        state.explorer().limits(),
    )))
}

#[derive(Deserialize)]
pub struct HistogramQuery {
    from_block: Option<u64>,
    to_block: Option<u64>,
    buckets: Option<u32>,
}

async fn histogram(
    State(state): State<AppState>,
    Query(query): Query<HistogramQuery>,
) -> Result<Json<HistogramResponse>, ApiError> {
    let span = match span_of(query.from_block, query.to_block)? {
        Some(span) => Some(span),
        None => indexed_span(&state).await?,
    };

    let Some(span) = span else {
        return Ok(Json(HistogramResponse::new(None, Vec::new())));
    };

    let buckets = query
        .buckets
        .unwrap_or(DEFAULT_BUCKETS)
        .clamp(1, MAX_BUCKETS);

    let histogram = state
        .explorer()
        .histogram(span, buckets)
        .await
        .map_err(|error| ApiError::upstream(error.to_string()))?;

    Ok(Json(HistogramResponse::new(Some(span), histogram)))
}

async fn indexed_span(state: &AppState) -> Result<Option<BlockRange>, ApiError> {
    let coverage = state
        .explorer()
        .coverage(BlockRange::new(0, u64::MAX))
        .await
        .map_err(|error| ApiError::upstream(error.to_string()))?;

    Ok(coverage
        .lowest_block()
        .zip(coverage.highest_block())
        .map(|(lowest, highest)| BlockRange::new(lowest, highest)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn histogram_query(query: &str) -> HistogramQuery {
        serde_urlencoded::from_str(query).expect("the query string is readable")
    }

    #[test]
    fn a_histogram_span_survives_the_query_string() {
        let query = histogram_query("from_block=25990217&to_block=25993415&buckets=6");

        assert_eq!(query.buckets, Some(6));
        assert_eq!(
            span_of(query.from_block, query.to_block)
                .unwrap()
                .map(|span| (span.from_block(), span.to_block())),
            Some((25_990_217, 25_993_415))
        );
    }

    #[test]
    fn a_bare_histogram_query_leaves_the_span_open() {
        let query = histogram_query("buckets=120");

        assert!(span_of(query.from_block, query.to_block).unwrap().is_none());
    }

    #[test]
    fn a_coverage_span_survives_the_query_string() {
        let query: SpanQuery =
            serde_urlencoded::from_str("from_block=10&to_block=20").expect("readable");

        assert_eq!(
            query.span().unwrap().map(|span| span.block_count()),
            Some(11)
        );
    }

    #[test]
    fn half_a_span_is_turned_away() {
        assert!(span_of(Some(10), None).is_err());
        assert!(span_of(Some(20), Some(10)).is_err());
    }
}
