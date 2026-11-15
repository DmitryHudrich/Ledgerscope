use utoipa::OpenApi;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};

use crate::routes;

struct ObserverAuth;

impl utoipa::Modify for ObserverAuth {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "observer_token",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("token")
                        .description(Some("The accounts.observer.token the agent posts with"))
                        .build(),
                ),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    modifiers(&ObserverAuth),
    info(
        title = "Ledgerscope API",
        description = "Address graphs and index coverage over indexed transactions of ethereum and its l2s.",
    ),
    paths(
        routes::chains,
        routes::graph,
        routes::low_level_graph,
        routes::coverage,
        routes::histogram,
        routes::auth::auth_config,
        routes::auth::register,
        routes::auth::login,
        routes::auth::logout,
        routes::auth::logout_everywhere,
        routes::auth::me,
        routes::auth::update_me,
        routes::auth::change_password,
        routes::teams::teams,
        routes::teams::create_team,
        routes::teams::team,
        routes::teams::rename_team,
        routes::teams::delete_team,
        routes::teams::add_member,
        routes::teams::change_role,
        routes::teams::remove_member,
        routes::canvases::canvases,
        routes::canvases::create_canvas,
        routes::canvases::canvas,
        routes::canvases::update_canvas,
        routes::canvases::delete_canvas,
        routes::canvases::shares,
        routes::canvases::share,
        routes::canvases::unshare,
        routes::canvases::links,
        routes::canvases::create_link,
        routes::canvases::revoke_link,
        routes::canvases::public_canvas,
        routes::labels::labels,
        routes::labels::create_label,
        routes::labels::update_label,
        routes::labels::delete_label,
        routes::rpc_nodes::rpc_nodes,
        routes::rpc_nodes::add_rpc_node,
        routes::rpc_nodes::delete_rpc_node,
        routes::tls::ingest,
        routes::tls::links,
        routes::tls::correlate,
        routes::tls::reset,
        routes::monitor::calls,
        routes::monitor::nodes,
        routes::monitor::scans,
        routes::monitor::stream,
        routes::info::usage,
    ),
    tags(
        (name = "chains", description = "Chains the server explores"),
        (name = "graph", description = "Address graph exploration"),
        (name = "coverage", description = "What the index already holds"),
        (name = "auth", description = "Accounts and cookie sessions"),
        (name = "teams", description = "Teams, members and roles"),
        (name = "canvases", description = "Saved canvases, sharing and public links"),
        (name = "labels", description = "Your own and your teams' address labels"),
        (name = "rpc-nodes", description = "Your own rpc nodes, asked first on your requests"),
        (name = "monitor", description = "Live rpc calls, node health and block scans"),
        (name = "info", description = "Disk and rpc traffic the server takes"),
        (name = "tls", description = "Passive tls traffic analysis of rpc clients and ip to wallet links"),
    ),
)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_route_reaches_the_spec() {
        let spec = serde_json::to_value(ApiDoc::openapi()).unwrap();

        assert!(spec["paths"]["/chains"]["get"].is_object());
        assert!(spec["paths"]["/graph"]["post"].is_object());
        assert!(spec["paths"]["/graph/low-level"]["post"].is_object());
        assert!(spec["paths"]["/coverage"]["get"].is_object());
        assert!(spec["paths"]["/coverage/histogram"]["get"].is_object());
        assert!(spec["paths"]["/auth/login"]["post"].is_object());
        assert!(spec["paths"]["/teams/{team_id}/members/{user_id}"]["patch"].is_object());
        assert!(spec["paths"]["/canvases/{canvas_id}"]["patch"].is_object());
        assert!(spec["paths"]["/public/canvases/{token}"]["get"].is_object());
        assert!(spec["paths"]["/labels"]["get"].is_object());
        assert!(spec["paths"]["/monitor/stream"]["get"].is_object());
        assert!(spec["paths"]["/rpc-nodes/{node_id}"]["delete"].is_object());
        assert!(spec["paths"]["/info/usage"]["get"].is_object());
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
