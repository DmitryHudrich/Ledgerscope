pub(crate) mod auth;
pub(crate) mod canvases;
pub(crate) mod info;
pub(crate) mod labels;
pub(crate) mod monitor;
pub(crate) mod rpc_nodes;
pub(crate) mod teams;
pub(crate) mod tls;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};
use utoipa_scalar::{Scalar, Servable};

use application::account::{AccountError, User};
use application::eth::{Exploration, ExploreError, ExploreRequest, GraphRoot, SpanChoice};
use domain::eth::BlockRange;

use crate::{
    auth::MaybeUser,
    dto::{
        ChainsResponse, CoverageResponse, DatasetResponse, GraphRequest, GraphResponse, HistogramResponse,
        LowLevelGraphResponse, RpcConfirmationResponse, ScopeRequest, parse_address,
    },
    openapi::ApiDoc,
    state::{AppState, ChainState},
};

const DEFAULT_BUCKETS: u32 = 120;
const MAX_BUCKETS: u32 = 1_000;

pub fn router(state: AppState) -> Router {
    let max_document_bytes = state.accounts().map_or(1024 * 1024, |accounts| {
        accounts.canvases.max_document_bytes()
    });

    Router::new()
        .merge(auth::router())
        .merge(teams::router())
        .merge(canvases::router(max_document_bytes))
        .merge(info::router())
        .merge(labels::router())
        .merge(monitor::router())
        .merge(rpc_nodes::router())
        .merge(tls::router())
        .merge(Scalar::with_url("/scalar", ApiDoc::openapi()))
        .route("/", get(index))
        .route("/openapi.json", get(openapi))
        .route("/chains", get(chains))
        .route("/graph", post(graph))
        .route("/graph/low-level", post(low_level_graph))
        .route("/coverage", get(coverage))
        .route("/head", get(head))
        .route("/coverage/histogram", get(histogram))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html("<h1>Hello, World!</h1>")
}

async fn openapi() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    error: String,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub(crate) fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub(crate) fn unavailable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, message)
    }

    pub(crate) fn too_many(message: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, message)
    }

    pub(crate) fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
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

impl From<AccountError> for ApiError {
    fn from(error: AccountError) -> Self {
        let status = match &error {
            AccountError::Invalid(_) => StatusCode::BAD_REQUEST,
            AccountError::Unauthenticated => StatusCode::UNAUTHORIZED,
            AccountError::Forbidden(_) => StatusCode::FORBIDDEN,
            AccountError::NotFound(_) => StatusCode::NOT_FOUND,
            AccountError::Conflict(_) => StatusCode::CONFLICT,
            AccountError::Storage(cause) => {
                tracing::error!("account storage failed: {cause}");
                return Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage failed, try again",
                );
            }
        };
        Self::new(status, error.to_string())
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

pub(crate) fn chain<'a>(
    state: &'a AppState,
    selector: Option<&str>,
) -> Result<&'a ChainState, ApiError> {
    state.chain(selector).ok_or_else(|| {
        ApiError::not_found(format!(
            "chain {} is not served here, see /chains",
            selector.unwrap_or_default()
        ))
    })
}

#[utoipa::path(
    get,
    path = "/chains",
    tag = "chains",
    responses(
        (status = OK, description = "Every chain this server explores", body = ChainsResponse),
    ),
)]
pub(crate) async fn chains(State(state): State<AppState>) -> Json<ChainsResponse> {
    Json(ChainsResponse::new(&state))
}

#[utoipa::path(
    post,
    path = "/graph",
    tag = "graph",
    request_body = GraphRequest,
    responses(
        (status = OK, description = "The address graph over the asked span", body = GraphResponse),
        (status = CONFLICT, description = "Part of the span is not indexed, resend with confirm_rpc", body = RpcConfirmationResponse),
        (status = BAD_REQUEST, description = "The request is malformed or over the limits", body = ErrorResponse),
        (status = NOT_FOUND, description = "The chain is not served here", body = ErrorResponse),
        (status = BAD_GATEWAY, description = "Clickhouse or the node did not answer", body = ErrorResponse),
    ),
)]
pub(crate) async fn graph(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Json(request): Json<GraphRequest>,
) -> Result<Response, ApiError> {
    let personal = state.personal_rpc(user.as_ref()).await;
    personal
        .scope(explore_graph(&state, user.as_ref(), request))
        .await
}

async fn explore_graph(
    state: &AppState,
    user: Option<&User>,
    request: GraphRequest,
) -> Result<Response, ApiError> {
    let chain = chain(
        state,
        request
            .chain
            .as_ref()
            .map(|chain| chain.selector())
            .as_deref(),
    )?;
    let asked = explore_request(&request)?;
    ensure_mined(chain, &request).await?;
    let span = asked.span();

    match chain.explorer().explore(asked).await? {
        Exploration::Graph(graph) => {
            let addresses = graph.nodes().iter().map(|node| *node.address()).collect();
            let mine = labels::graph_labels(state, user, chain.meta().chain_id, addresses).await;
            Ok(Json(GraphResponse::new(&graph, span, &mine)).into_response())
        }
        Exploration::RpcNeeded(plan) => Ok((
            StatusCode::CONFLICT,
            Json(RpcConfirmationResponse::new(&plan, span)),
        )
            .into_response()),
    }
}

async fn ensure_mined(chain: &ChainState, request: &GraphRequest) -> Result<(), ApiError> {
    let highest = highest_wanted(request);
    match chain.head_block().await {
        Some(head) if highest > head => Err(ApiError::bad_request(format!(
            "to_block {highest} is past the head of {} ({head})",
            chain.meta().name
        ))),
        _ => Ok(()),
    }
}

fn explore_request(request: &GraphRequest) -> Result<ExploreRequest, ApiError> {
    let spans = request_spans(request)?;
    let choices: Vec<SpanChoice> = request
        .ranges
        .iter()
        .map(|range| SpanChoice {
            source: range.source.into(),
            exact: range.exact,
        })
        .collect();
    if let ScopeRequest::Blocks = request.scope {
        return Ok(ExploreRequest::blocks(spans, request.confirm_rpc).with_choices(choices));
    }

    let mut roots = Vec::with_capacity(request.roots.len());
    for root in &request.roots {
        roots.push(GraphRoot::new(
            parse_address(&root.address).map_err(ApiError::bad_request)?,
            root.depth,
        ));
    }

    Ok(ExploreRequest::with_spans(roots, spans, request.confirm_rpc).with_choices(choices))
}

fn request_spans(request: &GraphRequest) -> Result<Vec<BlockRange>, ApiError> {
    if request.ranges.is_empty() {
        if request.from_block > request.to_block {
            return Err(ApiError::bad_request(
                "from_block must not be above to_block",
            ));
        }
        return Ok(vec![BlockRange::new(request.from_block, request.to_block)]);
    }

    let mut spans = Vec::with_capacity(request.ranges.len());
    for range in &request.ranges {
        if range.from_block > range.to_block {
            return Err(ApiError::bad_request(
                "each range's from_block must not be above its to_block",
            ));
        }
        spans.push(BlockRange::new(range.from_block, range.to_block));
    }
    Ok(spans)
}

fn highest_wanted(request: &GraphRequest) -> u64 {
    request
        .ranges
        .iter()
        .map(|range| range.to_block)
        .max()
        .unwrap_or(request.to_block)
}

#[utoipa::path(
    post,
    path = "/graph/low-level",
    tag = "graph",
    request_body = GraphRequest,
    responses(
        (status = OK, description = "The raw from-to graph over the asked span, one edge per transaction", body = LowLevelGraphResponse),
        (status = CONFLICT, description = "Part of the span is not indexed, resend with confirm_rpc", body = RpcConfirmationResponse),
        (status = BAD_REQUEST, description = "The request is malformed or over the limits", body = ErrorResponse),
        (status = NOT_FOUND, description = "The chain is not served here", body = ErrorResponse),
        (status = BAD_GATEWAY, description = "Clickhouse or the node did not answer", body = ErrorResponse),
    ),
)]
pub(crate) async fn low_level_graph(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Json(request): Json<GraphRequest>,
) -> Result<Response, ApiError> {
    let personal = state.personal_rpc(user.as_ref()).await;
    personal
        .scope(explore_low_level(&state, user.as_ref(), request))
        .await
}

async fn explore_low_level(
    state: &AppState,
    user: Option<&User>,
    request: GraphRequest,
) -> Result<Response, ApiError> {
    let chain = chain(
        state,
        request
            .chain
            .as_ref()
            .map(|chain| chain.selector())
            .as_deref(),
    )?;
    let asked = explore_request(&request)?;
    ensure_mined(chain, &request).await?;
    let span = asked.span();

    match chain.explorer().explore_low_level(asked).await? {
        Exploration::Graph(graph) => {
            let addresses = graph.actors().iter().map(|node| *node.address()).collect();
            let mine = labels::graph_labels(state, user, chain.meta().chain_id, addresses).await;
            Ok(Json(LowLevelGraphResponse::new(&graph, span, &mine)).into_response())
        }
        Exploration::RpcNeeded(plan) => Ok((
            StatusCode::CONFLICT,
            Json(RpcConfirmationResponse::new(&plan, span)),
        )
            .into_response()),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ChainQuery {
    chain: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct HeadResponse {
    chain_id: u64,
    head: u64,
}

#[utoipa::path(
    get,
    path = "/head",
    tag = "coverage",
    params(ChainQuery),
    responses(
        (status = OK, description = "The newest block the chain's nodes report right now", body = HeadResponse),
        (status = NOT_FOUND, description = "The chain is not served here", body = ErrorResponse),
        (status = BAD_GATEWAY, description = "No node told its head block", body = ErrorResponse),
    ),
)]
pub(crate) async fn head(
    State(state): State<AppState>,
    Query(query): Query<ChainQuery>,
) -> Result<Json<HeadResponse>, ApiError> {
    let chain = chain(&state, query.chain.as_deref())?;
    let head = chain
        .head_block()
        .await
        .ok_or_else(|| ApiError::upstream("no node told its head block"))?;
    Ok(Json(HeadResponse {
        chain_id: chain.meta().chain_id,
        head,
    }))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SpanQuery {
    from_block: Option<u64>,
    to_block: Option<u64>,
    chain: Option<String>,
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

#[utoipa::path(
    get,
    path = "/coverage",
    tag = "coverage",
    params(SpanQuery),
    responses(
        (status = OK, description = "What the index holds over the asked span", body = CoverageResponse),
        (status = BAD_REQUEST, description = "The span is malformed", body = ErrorResponse),
        (status = BAD_GATEWAY, description = "Clickhouse did not answer", body = ErrorResponse),
    ),
)]
pub(crate) async fn coverage(
    State(state): State<AppState>,
    Query(query): Query<SpanQuery>,
) -> Result<Json<CoverageResponse>, ApiError> {
    let chain = chain(&state, query.chain.as_deref())?;
    let span = query.span()?.unwrap_or(BlockRange::new(0, u64::MAX));
    let coverage = chain
        .explorer()
        .coverage(span)
        .await
        .map_err(|error| ApiError::upstream(error.to_string()))?;

    let dataset = if chain.explorer().has_dataset() {
        Some(DatasetResponse::new(chain.explorer().dataset_until().await))
    } else {
        None
    };

    Ok(Json(CoverageResponse::new(
        &coverage,
        chain.meta().chain_id,
        chain.explorer().persistent(),
        chain.head_block().await,
        chain.explorer().limits(),
        dataset,
    )))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HistogramQuery {
    from_block: Option<u64>,
    to_block: Option<u64>,
    buckets: Option<u32>,
    chain: Option<String>,
}

#[utoipa::path(
    get,
    path = "/coverage/histogram",
    tag = "coverage",
    params(HistogramQuery),
    responses(
        (status = OK, description = "Indexed blocks and tx counts per bucket", body = HistogramResponse),
        (status = BAD_REQUEST, description = "The span is malformed", body = ErrorResponse),
        (status = BAD_GATEWAY, description = "Clickhouse did not answer", body = ErrorResponse),
    ),
)]
pub(crate) async fn histogram(
    State(state): State<AppState>,
    Query(query): Query<HistogramQuery>,
) -> Result<Json<HistogramResponse>, ApiError> {
    let chain = chain(&state, query.chain.as_deref())?;
    let span = match span_of(query.from_block, query.to_block)? {
        Some(span) => Some(span),
        None => indexed_span(chain).await?,
    };

    let Some(span) = span else {
        return Ok(Json(HistogramResponse::new(None, Vec::new())));
    };

    let buckets = query
        .buckets
        .unwrap_or(DEFAULT_BUCKETS)
        .clamp(1, MAX_BUCKETS);

    let histogram = chain
        .explorer()
        .histogram(span, buckets)
        .await
        .map_err(|error| ApiError::upstream(error.to_string()))?;

    Ok(Json(HistogramResponse::new(Some(span), histogram)))
}

async fn indexed_span(chain: &ChainState) -> Result<Option<BlockRange>, ApiError> {
    let coverage = chain
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
