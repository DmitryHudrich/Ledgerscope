use std::sync::Arc;

use adapters::account::{Argon2Hasher, PgAccounts, RandomTokens};
use application::account::{
    AccountError, AuthService, CanvasPatch, CanvasService, LabelService, NewCanvas, NewLabel,
    RpcNodeProbe, RpcNodeService, TeamService,
};
use domain::{
    account::{CanvasAccess, LabelAccess, Owner, ShareAccess, TeamRole},
    eth::EthAddress,
};
use serde_json::json;
use sqlx::{AssertSqlSafe, Connection, PgConnection};

const POSTGRES_URL: &str = "LEDGERSCOPE_TEST_POSTGRES_URL";

struct HttpsOnly;

#[async_trait::async_trait]
impl RpcNodeProbe for HttpsOnly {
    async fn probe(&self, _chain_id: u64, url: &str) -> Result<String, String> {
        url.starts_with("https://")
            .then(|| url.trim_end_matches('/').to_owned())
            .ok_or_else(|| "the node did not pass the check".to_owned())
    }
}

struct Harness {
    auth: AuthService,
    teams: TeamService,
    canvases: CanvasService,
    labels: LabelService,
    rpc_nodes: RpcNodeService,
    admin_url: String,
    database: String,
}

impl Harness {
    async fn start() -> Option<Self> {
        let Ok(admin_url) = std::env::var(POSTGRES_URL) else {
            eprintln!("{POSTGRES_URL} is not set, skipping");
            return None;
        };

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let database = format!("ledgerscope_test_{nanos}");
        let mut admin = PgConnection::connect(&admin_url).await.unwrap();
        sqlx::query(AssertSqlSafe(format!("create database {database}")))
            .execute(&mut admin)
            .await
            .unwrap();

        let (base, _) = admin_url.rsplit_once('/').unwrap();
        let store = Arc::new(
            PgAccounts::connect(&format!("{base}/{database}"), 4)
                .await
                .unwrap(),
        );
        store.migrate().await.unwrap();

        let hasher = Arc::new(Argon2Hasher);
        let tokens = Arc::new(RandomTokens);
        Some(Self {
            auth: AuthService::new(
                store.clone(),
                store.clone(),
                hasher,
                tokens.clone(),
                time::Duration::days(1),
                true,
            ),
            teams: TeamService::new(store.clone(), store.clone()),
            canvases: CanvasService::new(
                store.clone(),
                store.clone(),
                store.clone(),
                tokens,
                64 * 1024,
            ),
            labels: LabelService::new(store.clone(), store.clone()),
            rpc_nodes: RpcNodeService::new(store, Arc::new(HttpsOnly), 2),
            admin_url,
            database,
        })
    }

    async fn stop(self) {
        drop(self.auth);
        drop(self.teams);
        drop(self.canvases);
        drop(self.labels);
        drop(self.rpc_nodes);
        let mut admin = PgConnection::connect(&self.admin_url).await.unwrap();
        sqlx::query(AssertSqlSafe(format!(
            "drop database if exists {} with (force)",
            self.database
        )))
        .execute(&mut admin)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn accounts_teams_canvases_and_labels_hold_together() {
    let Some(h) = Harness::start().await else {
        return;
    };

    let alice = h
        .auth
        .register("Alice@Example.com", "Alice", "alice password 1")
        .await
        .unwrap();
    assert_eq!(alice.user.email.as_str(), "alice@example.com");
    assert!(matches!(
        h.auth
            .register("alice@example.com", "Again", "another password")
            .await,
        Err(AccountError::Conflict(_))
    ));
    let bob = h
        .auth
        .register("bob@example.com", "Bob", "bob password 12")
        .await
        .unwrap();
    let carol = h
        .auth
        .register("carol@example.com", "Carol", "carol password 1")
        .await
        .unwrap();

    assert!(matches!(
        h.auth.login("alice@example.com", "wrong password").await,
        Err(AccountError::Invalid(_))
    ));
    assert!(matches!(
        h.auth.login("nobody@example.com", "whatever pass").await,
        Err(AccountError::Invalid(_))
    ));
    let again = h
        .auth
        .login(" ALICE@example.com ", "alice password 1")
        .await
        .unwrap();
    assert_eq!(
        h.auth.authenticate(&again.token).await.unwrap().unwrap().id,
        alice.user.id
    );
    h.auth.logout(&again.token).await.unwrap();
    assert!(h.auth.authenticate(&again.token).await.unwrap().is_none());
    assert!(h.auth.authenticate("garbage").await.unwrap().is_none());

    let renewed = h
        .auth
        .change_password(&alice.user, "alice password 1", "alice password 2")
        .await
        .unwrap();
    assert!(h.auth.authenticate(&alice.token).await.unwrap().is_none());
    assert!(h.auth.authenticate(&renewed.token).await.unwrap().is_some());
    let alice = renewed;

    let team = h.teams.create(&alice.user, "Investigations").await.unwrap();
    let team_id = team.team.id;
    h.teams
        .add_member(&alice.user, team_id, "bob@example.com", TeamRole::Editor)
        .await
        .unwrap();
    assert!(matches!(
        h.teams
            .add_member(&alice.user, team_id, "bob@example.com", TeamRole::Viewer)
            .await,
        Err(AccountError::Conflict(_))
    ));
    assert!(matches!(
        h.teams
            .add_member(&bob.user, team_id, "carol@example.com", TeamRole::Viewer)
            .await,
        Err(AccountError::Forbidden(_))
    ));
    assert!(matches!(
        h.teams
            .remove_member(&alice.user, team_id, alice.user.id)
            .await,
        Err(AccountError::Conflict(_))
    ));
    assert!(matches!(
        h.teams
            .change_role(&alice.user, team_id, alice.user.id, TeamRole::Admin)
            .await,
        Err(AccountError::Conflict(_))
    ));
    assert_eq!(h.teams.members(&bob.user, team_id).await.unwrap().len(), 2);
    assert!(matches!(
        h.teams.members(&carol.user, team_id).await,
        Err(AccountError::NotFound(_))
    ));
    assert_eq!(h.teams.list(&bob.user).await.unwrap()[0].members, 2);

    let personal = h
        .canvases
        .create(
            &alice.user,
            NewCanvas {
                owner: Owner::User(alice.user.id),
                title: "Mixer trail".to_owned(),
                chain_id: 1,
                document: json!({"roots": ["0xabc"]}),
            },
        )
        .await
        .unwrap();
    let shared = h
        .canvases
        .create(
            &bob.user,
            NewCanvas {
                owner: Owner::Team(team_id),
                title: "Team board".to_owned(),
                chain_id: 728_126_428,
                document: json!({}),
            },
        )
        .await
        .unwrap();
    assert_eq!(shared.access, CanvasAccess::Edit);
    assert!(matches!(
        h.canvases
            .create(
                &alice.user,
                NewCanvas {
                    owner: Owner::User(alice.user.id),
                    title: "Bad".to_owned(),
                    chain_id: 1,
                    document: json!([1, 2]),
                }
            )
            .await,
        Err(AccountError::Invalid(_))
    ));

    assert_eq!(h.canvases.list(&alice.user).await.unwrap().len(), 2);
    assert_eq!(h.canvases.list(&bob.user).await.unwrap().len(), 1);
    assert!(h.canvases.list(&carol.user).await.unwrap().is_empty());
    assert!(matches!(
        h.canvases.open(&carol.user, personal.summary.id).await,
        Err(AccountError::NotFound(_))
    ));

    h.canvases
        .share(
            &alice.user,
            personal.summary.id,
            "carol@example.com",
            ShareAccess::View,
        )
        .await
        .unwrap();
    let (seen, document) = h
        .canvases
        .open(&carol.user, personal.summary.id)
        .await
        .unwrap();
    assert_eq!(seen.access, CanvasAccess::View);
    assert_eq!(document, json!({"roots": ["0xabc"]}));
    assert!(matches!(
        h.canvases
            .update(&carol.user, personal.summary.id, CanvasPatch::default(), 1)
            .await,
        Err(AccountError::Forbidden(_))
    ));
    assert!(matches!(
        h.canvases
            .share(
                &bob.user,
                shared.summary.id,
                "carol@example.com",
                ShareAccess::Edit
            )
            .await,
        Err(AccountError::Forbidden(_))
    ));

    let updated = h
        .canvases
        .update(
            &alice.user,
            personal.summary.id,
            CanvasPatch {
                title: Some("Mixer trail, day 2".to_owned()),
                document: Some(json!({"roots": ["0xabc", "0xdef"]})),
                ..CanvasPatch::default()
            },
            1,
        )
        .await
        .unwrap();
    assert_eq!(updated.summary.version, 2);
    assert_eq!(updated.summary.title.as_str(), "Mixer trail, day 2");
    assert!(matches!(
        h.canvases
            .update(&alice.user, personal.summary.id, CanvasPatch::default(), 1)
            .await,
        Err(AccountError::Conflict(_))
    ));

    let (link, token) = h
        .canvases
        .create_link(&alice.user, personal.summary.id)
        .await
        .unwrap();
    let public = h.canvases.open_public(&token).await.unwrap();
    assert_eq!(public.document, json!({"roots": ["0xabc", "0xdef"]}));
    h.canvases
        .revoke_link(&alice.user, personal.summary.id, link.id)
        .await
        .unwrap();
    assert!(matches!(
        h.canvases.open_public(&token).await,
        Err(AccountError::NotFound(_))
    ));
    assert_eq!(
        h.canvases
            .links(&alice.user, personal.summary.id)
            .await
            .unwrap()
            .len(),
        1
    );

    h.canvases
        .unshare(&carol.user, personal.summary.id, carol.user.id)
        .await
        .unwrap();
    assert!(h.canvases.list(&carol.user).await.unwrap().is_empty());

    let wallet = EthAddress::from([7; 20]);
    h.labels
        .create(
            &alice.user,
            NewLabel {
                owner: Owner::User(alice.user.id),
                chain_id: 1,
                address: wallet,
                value: "Suspect".to_owned(),
                note: "seen at the mixer".to_owned(),
            },
        )
        .await
        .unwrap();
    let team_label = h
        .labels
        .create(
            &bob.user,
            NewLabel {
                owner: Owner::Team(team_id),
                chain_id: 1,
                address: wallet,
                value: "Exchange deposit".to_owned(),
                note: String::new(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        h.labels
            .create(
                &alice.user,
                NewLabel {
                    owner: Owner::User(alice.user.id),
                    chain_id: 1,
                    address: wallet,
                    value: "suspect".to_owned(),
                    note: String::new(),
                }
            )
            .await,
        Err(AccountError::Conflict(_))
    ));

    let for_alice = h.labels.for_graph(&alice.user, 1, &[wallet]).await.unwrap();
    let names: Vec<(&str, &str)> = for_alice[&wallet]
        .iter()
        .map(|label| (label.value(), label.source()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("Suspect", "you"),
            ("Exchange deposit", "team:Investigations")
        ]
    );
    assert_eq!(
        h.labels.for_graph(&bob.user, 1, &[wallet]).await.unwrap()[&wallet].len(),
        1
    );
    assert!(
        h.labels
            .for_graph(&carol.user, 1, &[wallet])
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        h.labels
            .for_graph(&alice.user, 10, &[wallet])
            .await
            .unwrap()
            .is_empty()
    );

    h.teams
        .change_role(&alice.user, team_id, bob.user.id, TeamRole::Viewer)
        .await
        .unwrap();
    let bobs = h.labels.visible(&bob.user, Some(1), None).await.unwrap();
    assert_eq!(bobs[0].access, LabelAccess::Read);
    assert!(matches!(
        h.labels
            .update(&bob.user, team_label.label.id, "Renamed", "")
            .await,
        Err(AccountError::Forbidden(_))
    ));
    h.labels
        .update(&alice.user, team_label.label.id, "Binance deposit", "")
        .await
        .unwrap();
    assert_eq!(
        h.labels
            .visible(&carol.user, None, None)
            .await
            .unwrap()
            .len(),
        0
    );

    h.teams
        .remove_member(&bob.user, team_id, bob.user.id)
        .await
        .unwrap();
    assert!(h.canvases.list(&bob.user).await.unwrap().is_empty());
    h.teams.delete(&alice.user, team_id).await.unwrap();
    assert_eq!(h.canvases.list(&alice.user).await.unwrap().len(), 1);
    assert_eq!(
        h.labels
            .visible(&alice.user, None, None)
            .await
            .unwrap()
            .len(),
        1
    );

    h.stop().await;
}

#[tokio::test]
async fn personal_rpc_nodes_stay_with_their_owner() {
    let Some(harness) = Harness::start().await else {
        return;
    };

    let alice = harness
        .auth
        .register("rpc-alice@example.com", "Alice", "correct horse battery")
        .await
        .unwrap()
        .user;
    let bob = harness
        .auth
        .register("rpc-bob@example.com", "Bob", "correct horse battery")
        .await
        .unwrap()
        .user;
    let nodes = &harness.rpc_nodes;

    let first = nodes
        .add(&alice, 1, " https://one.example/key/ ")
        .await
        .unwrap();
    assert_eq!(first.url, "https://one.example/key");
    nodes.add(&alice, 1, "https://two.example").await.unwrap();
    nodes.add(&alice, 10, "https://op.example").await.unwrap();

    assert!(matches!(
        nodes.add(&alice, 1, "https://three.example").await,
        Err(AccountError::Invalid(_))
    ));
    assert!(matches!(
        nodes.add(&alice, 10, "https://op.example/").await,
        Err(AccountError::Conflict(_))
    ));
    assert!(matches!(
        nodes.add(&alice, 10, "http://plain.example").await,
        Err(AccountError::Invalid(_))
    ));

    let by_chain = nodes.by_chain(&alice).await.unwrap();
    assert_eq!(by_chain[&1].len(), 2);
    assert_eq!(by_chain[&10], ["https://op.example"]);
    assert!(nodes.by_chain(&bob).await.unwrap().is_empty());

    assert!(matches!(
        nodes.delete(&bob, first.id).await,
        Err(AccountError::NotFound(_))
    ));
    nodes.delete(&alice, first.id).await.unwrap();
    assert_eq!(nodes.list(&alice, Some(1)).await.unwrap().len(), 1);

    harness.stop().await;
}
