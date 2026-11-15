mod networks;

use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

use anyhow::Context;

use adapters::account::{Argon2Hasher, PgAccounts, RandomTokens};
use adapters::eth::{
    Chainlist, ClickhouseTxRepository, Dialect, FleetChain, PersonalNodes, PersonalProbe,
    RedisTxCache, RpcFleet, RpcMonitor, RpcPool, RpcPoolConfig, RpcTxSource, TronTxSource,
    networks::known_name, tron,
};
use adapters::observe::PgObservations;
use application::account::{
    AuthService, CanvasService, LabelService, RpcNodeService, TeamService, User,
};
use application::observe::{ObservationStore, TlsCorrelationConfig};
use application::eth::{
    BuildBoard, CachingActorResolver, EthExplorer, ExploreLimits, FetchingTxIndex,
    JsonLabelProvider, ScanBoard, StoringEthTxSource,
    classificator::FulliestEthTxClassificator,
    ports::{
        ActorRepository, EthRpcSource, EthTxCache, EthTxIndex, EthTxRepository, EthTxSource,
        LabelProvider,
    },
};
use reqwest::{NoProxy, Proxy, Url};
use time::OffsetDateTime;

pub use networks::Network;

use crate::{
    auth::LoginThrottle,
    config::{AccountsConfig, Config, GraphConfig, StorageConfig},
};

const DEFAULT_NATIVE_SYMBOL: &str = "ETH";
const DEFAULT_NATIVE_DECIMALS: u8 = 18;
const TRON_NATIVE_SYMBOL: &str = "TRX";
const TRON_NATIVE_DECIMALS: u8 = 6;
const LOCAL_HOSTS: &str = "localhost,127.0.0.1,::1";

struct Storage {
    source: Arc<dyn EthTxSource>,
    index: Option<Arc<dyn EthTxIndex>>,
    actors: Option<Arc<dyn ActorRepository>>,
    repository: Option<Arc<dyn EthTxRepository>>,
}

struct SharedStorage {
    repository: Option<ClickhouseTxRepository>,
    cache: Option<RedisTxCache>,
}

impl SharedStorage {
    fn none() -> Self {
        Self {
            repository: None,
            cache: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChainMeta {
    pub chain_id: u64,
    pub family: Family,
    pub network: Network,
    pub name: String,
    pub slug: String,
    pub native_symbol: String,
    pub native_decimals: u8,
    pub l2_of: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Evm,
    Tron,
}

impl Family {
    fn of(chain_id: u64) -> Self {
        if tron::is_tron(chain_id) {
            Self::Tron
        } else {
            Self::Evm
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Evm => "evm",
            Self::Tron => "tron",
        }
    }

    fn dialect(self) -> Dialect {
        match self {
            Self::Evm => Dialect::EvmJsonRpc,
            Self::Tron => Dialect::TronHttp,
        }
    }
}

pub struct ChainState {
    meta: ChainMeta,
    explorer: Arc<EthExplorer>,
    rpc: Arc<dyn EthRpcSource>,
    index_repo: Option<Arc<dyn EthTxRepository>>,
    pool: Arc<RpcPool>,
}

impl ChainState {
    pub fn meta(&self) -> &ChainMeta {
        &self.meta
    }

    pub fn explorer(&self) -> &EthExplorer {
        &self.explorer
    }

    pub fn index_repo(&self) -> Option<&Arc<dyn EthTxRepository>> {
        self.index_repo.as_ref()
    }

    pub fn rpc_nodes(&self) -> usize {
        self.pool.len()
    }

    pub fn pool(&self) -> &RpcPool {
        &self.pool
    }

    pub async fn head_block(&self) -> Option<u64> {
        match self.rpc.head_block().await {
            Ok(head) => Some(head),
            Err(error) => {
                tracing::warn!(
                    "chain {}: the node did not tell its head block: {error}",
                    self.meta.chain_id
                );
                None
            }
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    chains: Arc<BTreeMap<u64, ChainState>>,
    default_chain: u64,
    accounts: Option<Arc<Accounts>>,
    monitor: Arc<RpcMonitor>,
    scans: Arc<ScanBoard>,
    builds: Arc<BuildBoard>,
    storage: Option<ClickhouseTxRepository>,
    started_at: OffsetDateTime,
}

pub struct Accounts {
    pub auth: AuthService,
    pub teams: TeamService,
    pub canvases: CanvasService,
    pub labels: LabelService,
    pub rpc_nodes: Option<RpcNodeService>,
    pub observations: Option<Arc<dyn ObservationStore>>,
    pub observer_token: Option<String>,
    pub correlation: TlsCorrelationConfig,
    pub cookie_secure: bool,
    pub throttle: LoginThrottle,
}

impl AppState {
    pub async fn from_config(config: &Config) -> anyhow::Result<Self> {
        let proxy = config.proxy.trim();
        let proxy_url = (!proxy.is_empty()).then(|| proxy.to_owned());
        let http_client = get_http_client(proxy_url, &config.no_proxy)?;

        let chainlist = chainlist(config, &http_client).await?;
        let plans = networks::plan(config, chainlist.as_deref(), &http_client).await;

        let mut fleet_chains = Vec::new();
        let mut metas = Vec::new();
        for (chain_id, plan) in &plans {
            let chain_id = *chain_id;
            let Some(pool) = pool_config(config, chainlist.as_deref(), chain_id, plan)? else {
                continue;
            };

            fleet_chains.push(FleetChain {
                pool,
                eager: config.configured(chain_id) || plan.network == Network::Devnet,
            });
            metas.push(meta(config, chainlist.as_deref(), chain_id, plan));
        }

        let fleet = RpcFleet::start(
            http_client.clone(),
            chainlist,
            fleet_chains,
            Duration::from_secs(config.rpc.rescan_interval_secs()),
        )
        .await;

        let default_chain = config.chains.default;
        anyhow::ensure!(
            fleet
                .pool(default_chain)
                .is_some_and(|pool| !pool.is_empty()),
            "no rpc node of the default chain {default_chain} answered"
        );

        let shared = shared_storage(&config.storage).await?;
        let mut labels = labels(config, &metas)?;
        let classificator = Arc::new(FulliestEthTxClassificator::new());

        let monitor = Arc::new(RpcMonitor::default());
        let scans = Arc::new(ScanBoard::default());
        let builds = Arc::new(BuildBoard::default());

        let mut chains = BTreeMap::new();
        for meta in metas {
            let chain_id = meta.chain_id;
            let pool = fleet
                .pool(chain_id)
                .expect("every chain got a pool in the fleet");
            pool.watch(monitor.clone());
            let (tx_source, rpc_source) = sources(meta.family, pool.clone());
            let storage = if meta.network == Network::Devnet {
                storage(&SharedStorage::none(), chain_id, tx_source)
            } else {
                storage(&shared, chain_id, tx_source)
            };
            let label_provider = Arc::new(
                labels
                    .remove(&chain_id)
                    .unwrap_or_else(JsonLabelProvider::empty),
            ) as Arc<dyn LabelProvider>;

            let actor_resolver = Arc::new(CachingActorResolver::new(
                rpc_source.clone(),
                storage.actors,
                label_provider.clone(),
            ));

            let index_repo = storage.repository.clone();
            let index = storage.index.unwrap_or_else(|| {
                Arc::new(FetchingTxIndex::new(storage.source.clone())) as Arc<dyn EthTxIndex>
            });

            let explorer = Arc::new(
                EthExplorer::new(
                    index,
                    storage.source,
                    classificator.clone(),
                    actor_resolver,
                    label_provider,
                    limits(&config.graph),
                )
                .with_scans(scans.for_chain(chain_id))
                .with_builds(builds.for_chain(chain_id)),
            );

            chains.insert(
                chain_id,
                ChainState {
                    meta,
                    explorer,
                    rpc: rpc_source,
                    index_repo,
                    pool,
                },
            );
        }

        tracing::info!(
            "serving {} chains, {default_chain} by default",
            chains.len()
        );

        Ok(Self {
            chains: Arc::new(chains),
            default_chain,
            accounts: accounts(&config.accounts, &fleet, &http_client).await,
            monitor,
            scans,
            builds,
            storage: shared.repository.clone(),
            started_at: OffsetDateTime::now_utc(),
        })
    }

    pub fn accounts(&self) -> Option<&Arc<Accounts>> {
        self.accounts.as_ref()
    }

    pub async fn personal_rpc(&self, user: Option<&User>) -> PersonalNodes {
        let (Some(service), Some(user)) = (
            self.accounts
                .as_ref()
                .and_then(|accounts| accounts.rpc_nodes.as_ref()),
            user,
        ) else {
            return PersonalNodes::default();
        };
        match service.by_chain(user).await {
            Ok(urls) => PersonalNodes::new(urls),
            Err(error) => {
                tracing::warn!("going on without the personal rpc nodes: {error}");
                PersonalNodes::default()
            }
        }
    }

    pub fn monitor(&self) -> &RpcMonitor {
        &self.monitor
    }

    pub fn scans(&self) -> &ScanBoard {
        &self.scans
    }

    pub fn builds(&self) -> &BuildBoard {
        &self.builds
    }

    pub fn storage(&self) -> Option<&ClickhouseTxRepository> {
        self.storage.as_ref()
    }

    pub fn started_at(&self) -> OffsetDateTime {
        self.started_at
    }

    pub fn default_chain(&self) -> u64 {
        self.default_chain
    }

    pub fn chains(&self) -> impl Iterator<Item = &ChainState> {
        self.chains.values()
    }

    pub fn chain(&self, selector: Option<&str>) -> Option<&ChainState> {
        let Some(selector) = selector
            .map(str::trim)
            .filter(|selector| !selector.is_empty())
        else {
            return self.chains.get(&self.default_chain);
        };

        if let Ok(chain_id) = selector.parse::<u64>() {
            return self.chains.get(&chain_id);
        }

        let slug = adapters::eth::chainlist::slugify(selector);
        self.chains.values().find(|chain| chain.meta.slug == slug)
    }
}

async fn chainlist(
    config: &Config,
    http_client: &reqwest::Client,
) -> anyhow::Result<Option<Arc<Chainlist>>> {
    if !config.needs_chainlist() {
        return Ok(None);
    }

    let url = Url::parse(config.rpc.chainlist_url()).with_context(|| {
        format!(
            "rpc.chainlist_url is not a valid URL: {}",
            config.rpc.chainlist_url()
        )
    })?;

    match Chainlist::load(http_client.clone(), url).await {
        Ok(chainlist) => Ok(Some(chainlist)),
        Err(error) => {
            tracing::warn!("going on without chainlist: {error}");
            Ok(None)
        }
    }
}

fn sources(family: Family, pool: Arc<RpcPool>) -> (Arc<dyn EthTxSource>, Arc<dyn EthRpcSource>) {
    match family {
        Family::Evm => {
            let source = Arc::new(RpcTxSource::new(pool));
            (source.clone(), source)
        }
        Family::Tron => {
            let source = Arc::new(TronTxSource::new(pool));
            (source.clone(), source)
        }
    }
}

fn pool_config(
    config: &Config,
    chainlist: Option<&Chainlist>,
    chain_id: u64,
    plan: &networks::Plan,
) -> anyhow::Result<Option<RpcPoolConfig>> {
    let rpc = config.chain_rpc(chain_id);
    let family = plan.family;
    let devnet = plan.network == Network::Devnet;
    let seeds = match (devnet, family) {
        (true, _) => Vec::new(),
        (false, Family::Evm) => adapters::eth::networks::public_nodes(chain_id),
        (false, Family::Tron) => tron::public_nodes(chain_id),
    };
    let listed = chainlist
        .filter(|_| !devnet)
        .map_or(0, |chainlist| chainlist.rpc_urls(chain_id).len());
    let public = rpc.chainlist && !devnet && listed + seeds.len() > 0;

    if rpc.own.is_empty() && plan.own.is_empty() && !public {
        anyhow::ensure!(
            chain_id != config.chains.default,
            "the default chain {chain_id} has no rpc to talk to"
        );
        tracing::warn!("chain {chain_id} has no rpc to talk to, leaving it out");
        return Ok(None);
    }

    let own = rpc
        .own
        .iter()
        .map(|url| {
            Url::parse(url)
                .with_context(|| format!("chain {chain_id} has an invalid rpc URL: {url}"))
        })
        .chain(plan.own.iter().cloned().map(Ok))
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(Some(RpcPoolConfig {
        chain_id,
        dialect: family.dialect(),
        rps: rpc.rps,
        own,
        seeds,
        exclude: rpc.exclude,
        max_public: if public { rpc.chainlist_max_nodes } else { 0 },
    }))
}

fn meta(
    config: &Config,
    chainlist: Option<&Chainlist>,
    chain_id: u64,
    plan: &networks::Plan,
) -> ChainMeta {
    let network = config.network(chain_id);
    let listed = chainlist
        .filter(|_| plan.network != Network::Devnet)
        .and_then(|chainlist| chainlist.chain(chain_id));
    let fallback_name = plan
        .name
        .clone()
        .or_else(|| known_name(chain_id).map(str::to_owned));
    let pick = |own: Option<&String>, listed: Option<&String>| {
        own.or(listed)
            .filter(|value| !value.trim().is_empty())
            .cloned()
    };

    let name = pick(
        network.and_then(|network| network.name.as_ref()),
        listed.as_ref().map(|chain| &chain.name),
    )
    .or_else(|| fallback_name.clone())
    .unwrap_or_else(|| format!("Chain {chain_id}"));

    let slug = pick(
        network.and_then(|network| network.slug.as_ref()),
        listed.as_ref().map(|chain| &chain.slug),
    )
    .or(fallback_name)
    .map(|slug| adapters::eth::chainlist::slugify(&slug))
    .unwrap_or_else(|| chain_id.to_string());

    let family = plan.family;
    let (fallback_symbol, fallback_decimals) = match family {
        Family::Evm => (DEFAULT_NATIVE_SYMBOL, DEFAULT_NATIVE_DECIMALS),
        Family::Tron => (TRON_NATIVE_SYMBOL, TRON_NATIVE_DECIMALS),
    };

    let native_symbol = pick(
        network.and_then(|network| network.native_symbol.as_ref()),
        listed.as_ref().map(|chain| &chain.native_symbol),
    )
    .unwrap_or_else(|| fallback_symbol.to_owned());

    let native_decimals = network
        .and_then(|network| network.native_decimals)
        .or(listed.as_ref().map(|chain| chain.native_decimals))
        .unwrap_or(fallback_decimals);

    let l2_of = listed.as_ref().and_then(|chain| chain.l2_of);

    ChainMeta {
        chain_id,
        family,
        network: plan.network,
        name,
        slug,
        native_symbol,
        native_decimals,
        l2_of,
    }
}

fn labels(
    config: &Config,
    metas: &[ChainMeta],
) -> anyhow::Result<std::collections::HashMap<u64, JsonLabelProvider>> {
    let path = config.labels.etherscan_json_path.trim();
    if path.is_empty() {
        tracing::info!("labels.etherscan_json_path is empty, running without labels");
        return Ok(Default::default());
    }

    let chain_ids: Vec<u64> = metas.iter().map(|meta| meta.chain_id).collect();
    JsonLabelProvider::read_chains(Path::new(path), &chain_ids)
        .with_context(|| format!("failed to read the labels from {path}"))
}

async fn accounts(
    config: &AccountsConfig,
    fleet: &Arc<RpcFleet>,
    http_client: &reqwest::Client,
) -> Option<Arc<Accounts>> {
    if !config.enabled {
        tracing::info!("accounts.enabled is off, running without accounts");
        return None;
    }

    let store = match PgAccounts::connect(&config.postgres_url, config.max_connections).await {
        Ok(store) => Arc::new(store),
        Err(error) => {
            tracing::warn!("running without accounts, postgres is not available: {error}");
            return None;
        }
    };
    if let Err(error) = store.migrate().await {
        tracing::warn!("running without accounts: {error}");
        return None;
    }

    let observer = &config.observer;
    let observations = observer.enabled.then(|| {
        Arc::new(PgObservations::new(store.pool().clone())) as Arc<dyn ObservationStore>
    });
    let observer_token = observer
        .enabled
        .then(|| observer.token.trim().to_owned())
        .filter(|token| !token.is_empty());
    if observer.enabled && observer_token.is_none() {
        tracing::warn!("accounts.observer.enabled is on but the token is empty, ingest is closed");
    }

    let tokens = Arc::new(RandomTokens);
    let accounts = Arc::new(Accounts {
        auth: AuthService::new(
            store.clone(),
            store.clone(),
            Arc::new(Argon2Hasher),
            tokens.clone(),
            time::Duration::days(i64::from(config.session_ttl_days)),
            config.registration,
        ),
        teams: TeamService::new(store.clone(), store.clone()),
        canvases: CanvasService::new(
            store.clone(),
            store.clone(),
            store.clone(),
            tokens,
            config.max_canvas_bytes,
        ),
        labels: LabelService::new(store.clone(), store.clone()),
        rpc_nodes: config.personal_rpc.enabled.then(|| {
            RpcNodeService::new(
                store,
                Arc::new(PersonalProbe::new(
                    fleet.clone(),
                    http_client.clone(),
                    config.personal_rpc.allow_private_hosts,
                )),
                config.personal_rpc.max_nodes_per_chain,
            )
        }),
        observations,
        observer_token,
        correlation: TlsCorrelationConfig {
            window_secs: observer.window_secs,
            skew_secs: observer.skew_secs,
        },
        cookie_secure: config.cookie_secure,
        throttle: LoginThrottle::default(),
    });

    let sweeper = accounts.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3600));
        loop {
            tick.tick().await;
            match sweeper.auth.purge_expired().await {
                Ok(0) => {}
                Ok(purged) => tracing::info!("purged {purged} expired sessions"),
                Err(error) => tracing::warn!("could not purge expired sessions: {error}"),
            }
        }
    });

    tracing::info!(
        "accounts are on, registration is {}",
        if config.registration {
            "open"
        } else {
            "closed"
        }
    );
    Some(accounts)
}

fn limits(graph: &GraphConfig) -> ExploreLimits {
    ExploreLimits {
        max_roots: graph.max_roots,
        max_depth: graph.max_depth,
        max_nodes: graph.max_nodes,
        max_edges: graph.max_edges,
        max_blocks: graph.max_blocks,
    }
}

async fn shared_storage(storage: &StorageConfig) -> anyhow::Result<SharedStorage> {
    if !storage.persist_txs {
        tracing::info!("storage.persist_txs is off, every request goes to the rpc");
        return Ok(SharedStorage::none());
    }

    let clickhouse = &storage.clickhouse;
    let url = Url::parse(&clickhouse.url).with_context(|| {
        format!(
            "storage.clickhouse.url is not a valid URL: {}",
            clickhouse.url
        )
    })?;

    let repository = ClickhouseTxRepository::new(
        reqwest::Client::new(),
        url,
        clickhouse.database.clone(),
        clickhouse.user.clone(),
        clickhouse.password.clone(),
    );

    repository
        .ensure_schema()
        .await
        .context("failed to prepare the clickhouse schema")?;

    Ok(SharedStorage {
        repository: Some(repository),
        cache: tx_cache(&storage.redis.url).await,
    })
}

fn storage(shared: &SharedStorage, chain_id: u64, upstream: Arc<dyn EthTxSource>) -> Storage {
    let Some(repository) = &shared.repository else {
        return Storage {
            source: upstream,
            index: None,
            actors: None,
            repository: None,
        };
    };

    let repository = Arc::new(repository.for_chain(chain_id));
    let cache = shared
        .cache
        .as_ref()
        .map(|cache| Arc::new(cache.for_chain(chain_id)) as Arc<dyn EthTxCache>);

    Storage {
        source: Arc::new(StoringEthTxSource::new(upstream, repository.clone(), cache)),
        index: Some(repository.clone() as Arc<dyn EthTxIndex>),
        repository: Some(repository.clone() as Arc<dyn EthTxRepository>),
        actors: Some(repository as Arc<dyn ActorRepository>),
    }
}

async fn tx_cache(url: &str) -> Option<RedisTxCache> {
    if url.is_empty() {
        tracing::info!("storage.redis.url is empty, running without a cache");
        return None;
    }

    match RedisTxCache::connect(url).await {
        Ok(cache) => Some(cache),
        Err(error) => {
            tracing::warn!("running without a cache, redis is not available: {error}");
            None
        }
    }
}

fn get_http_client(proxy_url: Option<String>, no_proxy: &str) -> anyhow::Result<reqwest::Client> {
    let mut client = reqwest::ClientBuilder::new();

    if let Some(proxy_url) = proxy_url {
        tracing::info!(
            "every outgoing request goes through the proxy, except {LOCAL_HOSTS},{no_proxy}"
        );
        client = client.proxy(
            Proxy::all(&proxy_url)
                .with_context(|| format!("proxy is not a valid proxy URL: {}", proxy_url))?
                .no_proxy(NoProxy::from_string(&format!("{LOCAL_HOSTS},{no_proxy}"))),
        );
    }

    client.build().context("failed to build the HTTP client")
}
