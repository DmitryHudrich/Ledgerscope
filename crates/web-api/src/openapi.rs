use utoipa::OpenApi;

use crate::routes;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Ledgerscope API",
        description = "Address graphs and index coverage over indexed eth transactions.",
    ),
    paths(routes::graph, routes::coverage, routes::histogram),
    tags(
        (name = "graph", description = "Address graph exploration"),
        (name = "coverage", description = "What the index already holds"),
    ),
)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_route_reaches_the_spec() {
        let spec = serde_json::to_value(ApiDoc::openapi()).unwrap();

        assert!(spec["paths"]["/graph"]["post"].is_object());
        assert!(spec["paths"]["/coverage"]["get"].is_object());
        assert!(spec["paths"]["/coverage/histogram"]["get"].is_object());
        assert!(spec["components"]["schemas"]["GraphResponse"].is_object());
        assert_eq!(
            spec["paths"]["/coverage/histogram"]["get"]["parameters"][2]["name"],
            serde_json::json!("buckets")
        );
    }

    #[test]
    fn a_node_carries_its_actor_into_the_spec() {
        let spec = serde_json::to_value(ApiDoc::openapi()).unwrap();

        assert!(spec["components"]["schemas"]["NodeResponse"].is_object());
        assert!(spec["components"]["schemas"]["ActorResponse"].is_object());
    }
}
