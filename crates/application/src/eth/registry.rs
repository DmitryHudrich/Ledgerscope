use std::{
    collections::{HashMap, HashSet},
    io,
    sync::{Arc, LazyLock, PoisonError, RwLock},
    time::{Duration, Instant},
};

use alloy_primitives::{Address, B256, Bytes, U256, address, keccak256};
use futures::StreamExt;

use domain::eth::{BlockRef, EthAddress, PoolCandidate, PoolInfo, PoolKey, SwapFamily, TagSource};

use crate::eth::ports::{EthRpcSource, PoolRegistry, PoolRepository};

const ASKING_CONCURRENCY: usize = 8;
const SOURCE_VERSION: u32 = 1;
const MAX_FEE: u32 = 0xff_ffff;
const DYNAMIC_FEE_FLAG: u32 = 0x80_0000;
const WORD: usize = 32;
const NOT_A_POOL_TTL: Duration = Duration::from_secs(600);
const UNKNOWN_TTL: Duration = Duration::from_secs(60);
const MAX_REMEMBERED_ABSENT: usize = 50_000;

static FACTORY: LazyLock<[u8; 4]> = LazyLock::new(|| selector("factory()"));
static TOKEN0: LazyLock<[u8; 4]> = LazyLock::new(|| selector("token0()"));
static TOKEN1: LazyLock<[u8; 4]> = LazyLock::new(|| selector("token1()"));
static FEE: LazyLock<[u8; 4]> = LazyLock::new(|| selector("fee()"));
static GET_PAIR: LazyLock<[u8; 4]> = LazyLock::new(|| selector("getPair(address,address)"));
static GET_POOL: LazyLock<[u8; 4]> = LazyLock::new(|| selector("getPool(address,address,uint24)"));
static POOL_KEYS: LazyLock<[u8; 4]> = LazyLock::new(|| selector("poolKeys(bytes25)"));

fn selector(signature: &str) -> [u8; 4] {
    let hash = keccak256(signature.as_bytes());
    [hash[0], hash[1], hash[2], hash[3]]
}

struct Listed {
    address: Address,
    protocol: &'static str,
    fee: Option<u32>,
}

const V2_FACTORIES: &[Listed] = &[
    Listed {
        address: address!("5C69bEe701ef814a2B6a3EDD4B1652CB9cc5aA6f"),
        protocol: "Uniswap V2",
        fee: Some(3000),
    },
    Listed {
        address: address!("C0AEe478e3658e2610c5F7A4A2E1777cE9e4f2Ac"),
        protocol: "SushiSwap V2",
        fee: Some(3000),
    },
    Listed {
        address: address!("1097053Fd2ea711dad45caCcc45EfF7548fCB362"),
        protocol: "PancakeSwap V2",
        fee: Some(2500),
    },
    Listed {
        address: address!("cA143Ce32Fe78f1f7019d7d551a6402fC5350c73"),
        protocol: "PancakeSwap V2",
        fee: Some(2500),
    },
];

const V3_FACTORIES: &[Listed] = &[
    Listed {
        address: address!("1F98431c8aD98523631AE4a59f267346ea31F984"),
        protocol: "Uniswap V3",
        fee: None,
    },
    Listed {
        address: address!("33128a8fC17869897dcE68Ed026d694621f6FDfD"),
        protocol: "Uniswap V3",
        fee: None,
    },
    Listed {
        address: address!("dB1d10011AD0Ff90774D0C6Bb92e5C5c8b4461F7"),
        protocol: "Uniswap V3",
        fee: None,
    },
    Listed {
        address: address!("bACEB8eC6b9355Dfc0269C18bac9d6E2Bdc29C4F"),
        protocol: "SushiSwap V3",
        fee: None,
    },
];

struct Manager {
    address: Address,
    position_manager: Address,
}

const V4_MANAGERS: &[Manager] = &[Manager {
    address: address!("000000000004444c5dc75cb358380d2e3de08a90"),
    position_manager: address!("bD216513d74C8cf14cf4747E6AaA6420FF64ee9e"),
}];

const POOL_KEY_WORDS: usize = 5;
const POOL_ID_PREFIX: usize = 25;

fn listed<'a>(table: &'a [Listed], factory: &EthAddress) -> Option<&'a Listed> {
    table
        .iter()
        .find(|entry| EthAddress::from(entry.address) == *factory)
}

#[derive(Clone, Copy)]
enum Doubt {
    NotAPool,
    Unknown,
}

enum Verdict {
    Pool(PoolInfo),
    Doubted(Doubt),
}

enum Entry {
    Pool(PoolInfo),
    Absent(Instant),
}

enum Asked {
    Word(B256),
    Refused,
    Failed,
}

impl Asked {
    fn settle(self) -> Result<B256, Doubt> {
        match self {
            Asked::Word(word) => Ok(word),
            Asked::Refused => Err(Doubt::NotAPool),
            Asked::Failed => Err(Doubt::Unknown),
        }
    }

    fn address(self) -> Result<EthAddress, Doubt> {
        let word = self.settle()?;
        if word[..12].iter().any(|byte| *byte != 0) {
            return Err(Doubt::NotAPool);
        }
        let address = EthAddress::from_word(word);
        if address == EthAddress::ZERO {
            return Err(Doubt::NotAPool);
        }
        Ok(address)
    }

    fn fee(self) -> Result<u32, Doubt> {
        let word = self.settle()?;
        u32::try_from(U256::from_be_bytes(word.0))
            .ok()
            .filter(|fee| *fee <= MAX_FEE)
            .ok_or(Doubt::NotAPool)
    }
}

fn answered_with_revert(error: &io::Error) -> bool {
    error.to_string().contains("execution reverted")
}

fn calldata(selector: &[u8; 4], arguments: &[B256]) -> Vec<u8> {
    let mut data = Vec::with_capacity(4 + WORD * arguments.len());
    data.extend_from_slice(selector);
    for argument in arguments {
        data.extend_from_slice(argument.as_slice());
    }
    data
}

pub struct ChainPoolRegistry {
    rpc: Arc<dyn EthRpcSource>,
    repository: Option<Arc<dyn PoolRepository>>,
    known: RwLock<HashMap<PoolKey, Entry>>,
    not_a_pool_ttl: Duration,
    unknown_ttl: Duration,
}

impl ChainPoolRegistry {
    pub fn new(rpc: Arc<dyn EthRpcSource>, repository: Option<Arc<dyn PoolRepository>>) -> Self {
        Self {
            rpc,
            repository,
            known: RwLock::new(HashMap::new()),
            not_a_pool_ttl: NOT_A_POOL_TTL,
            unknown_ttl: UNKNOWN_TTL,
        }
    }

    pub fn with_backoff(mut self, not_a_pool: Duration, unknown: Duration) -> Self {
        self.not_a_pool_ttl = not_a_pool;
        self.unknown_ttl = unknown;
        self
    }

    fn remembered(&self, key: &PoolKey) -> Option<Option<PoolInfo>> {
        let known = self.known.read().unwrap_or_else(PoisonError::into_inner);

        match known.get(key)? {
            Entry::Pool(info) => Some(Some(info.clone())),
            Entry::Absent(until) if *until > Instant::now() => Some(None),
            Entry::Absent(_) => None,
        }
    }

    fn remember_pool(&self, info: PoolInfo) {
        self.known
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(*info.key(), Entry::Pool(info));
    }

    fn remember_doubt(&self, key: PoolKey, doubt: Doubt) {
        let ttl = match doubt {
            Doubt::NotAPool => self.not_a_pool_ttl,
            Doubt::Unknown => self.unknown_ttl,
        };

        let mut known = self.known.write().unwrap_or_else(PoisonError::into_inner);

        let absent = known
            .values()
            .filter(|entry| matches!(entry, Entry::Absent(_)))
            .count();
        if absent >= MAX_REMEMBERED_ABSENT {
            known.retain(|_, entry| matches!(entry, Entry::Pool(_)));
        }

        known
            .entry(key)
            .and_modify(|entry| {
                if matches!(entry, Entry::Absent(_)) {
                    *entry = Entry::Absent(Instant::now() + ttl);
                }
            })
            .or_insert_with(|| Entry::Absent(Instant::now() + ttl));
    }

    async fn recall(&self, wanted: &[PoolCandidate]) -> Vec<PoolInfo> {
        let Some(repository) = self.repository.as_ref() else {
            return Vec::new();
        };

        let keys: Vec<PoolKey> = wanted.iter().map(|candidate| *candidate.key()).collect();

        match repository.pools(&keys).await {
            Ok(pools) => pools,
            Err(error) => {
                tracing::warn!("storage forgot what it knew about pools: {error}");
                Vec::new()
            }
        }
    }

    async fn store(&self, pools: &[PoolInfo]) {
        let Some(repository) = self.repository.as_ref() else {
            return;
        };

        if pools.is_empty() {
            return;
        }

        if let Err(error) = repository.remember(pools).await {
            tracing::warn!("could not write {} pools down: {error}", pools.len());
        }
    }

    async fn ask_bytes(
        &self,
        to: &EthAddress,
        selector: &[u8; 4],
        arguments: &[B256],
    ) -> Result<Bytes, Doubt> {
        match self
            .rpc
            .call(to, &calldata(selector, arguments), BlockRef::Latest)
            .await
        {
            Ok(returned) => Ok(returned),
            Err(error) if answered_with_revert(&error) => Err(Doubt::NotAPool),
            Err(error) => {
                tracing::debug!("calling {to} failed, leaving its pool status open: {error}");
                Err(Doubt::Unknown)
            }
        }
    }

    async fn ask(&self, to: &EthAddress, selector: &[u8; 4], arguments: &[B256]) -> Asked {
        match self.ask_bytes(to, selector, arguments).await {
            Ok(returned) if returned.len() >= WORD => {
                Asked::Word(B256::from_slice(&returned[..WORD]))
            }
            Ok(_) | Err(Doubt::NotAPool) => Asked::Refused,
            Err(Doubt::Unknown) => Asked::Failed,
        }
    }

    async fn verify(&self, candidate: PoolCandidate) -> Verdict {
        let checked = match candidate.family() {
            SwapFamily::UniswapV2 => self.check_v2(&candidate).await,
            SwapFamily::UniswapV3 => self.check_v3(&candidate).await,
            SwapFamily::UniswapV4 => self.check_v4(&candidate).await,
        };

        match checked {
            Ok(info) => Verdict::Pool(info),
            Err(doubt) => Verdict::Doubted(doubt),
        }
    }

    async fn check_v2(&self, candidate: &PoolCandidate) -> Result<PoolInfo, Doubt> {
        let pool = *candidate.key().address();
        let (factory, token0, token1) = tokio::join!(
            self.ask(&pool, &FACTORY, &[]),
            self.ask(&pool, &TOKEN0, &[]),
            self.ask(&pool, &TOKEN1, &[]),
        );
        let (factory, token0, token1) = (factory.address()?, token0.address()?, token1.address()?);
        let known = listed(V2_FACTORIES, &factory).ok_or(Doubt::NotAPool)?;

        let pair = self
            .ask(
                &factory,
                &GET_PAIR,
                &[token0.address().into_word(), token1.address().into_word()],
            )
            .await
            .address()?;
        if pair != pool {
            return Err(Doubt::NotAPool);
        }

        Ok(PoolInfo::new(
            *candidate.key(),
            SwapFamily::UniswapV2,
            known.protocol.to_owned(),
            vec![token0, token1],
            known.fee,
            Some(factory),
            TagSource::heuristic("pool_registry.uniswap_v2", SOURCE_VERSION),
        ))
    }

    async fn check_v3(&self, candidate: &PoolCandidate) -> Result<PoolInfo, Doubt> {
        let pool = *candidate.key().address();
        let (factory, token0, token1, fee) = tokio::join!(
            self.ask(&pool, &FACTORY, &[]),
            self.ask(&pool, &TOKEN0, &[]),
            self.ask(&pool, &TOKEN1, &[]),
            self.ask(&pool, &FEE, &[]),
        );
        let (factory, token0, token1, fee) = (
            factory.address()?,
            token0.address()?,
            token1.address()?,
            fee.fee()?,
        );

        let known = listed(V3_FACTORIES, &factory).ok_or(Doubt::NotAPool)?;

        let registered = self
            .ask(
                &factory,
                &GET_POOL,
                &[
                    token0.address().into_word(),
                    token1.address().into_word(),
                    B256::from(U256::from(fee)),
                ],
            )
            .await
            .address()?;
        if registered != pool {
            return Err(Doubt::NotAPool);
        }

        Ok(PoolInfo::new(
            *candidate.key(),
            SwapFamily::UniswapV3,
            known.protocol.to_owned(),
            vec![token0, token1],
            Some(fee),
            Some(factory),
            TagSource::heuristic("pool_registry.uniswap_v3", SOURCE_VERSION),
        ))
    }

    async fn check_v4(&self, candidate: &PoolCandidate) -> Result<PoolInfo, Doubt> {
        let manager = *candidate.key().address();
        let (Some(id), Some(known)) = (
            candidate.key().pool_id().copied(),
            V4_MANAGERS
                .iter()
                .find(|known| EthAddress::from(known.address) == manager),
        ) else {
            return Err(Doubt::NotAPool);
        };

        let mut prefix = [0u8; WORD];
        prefix[..POOL_ID_PREFIX].copy_from_slice(&id[..POOL_ID_PREFIX]);

        let returned = self
            .ask_bytes(
                &EthAddress::from(known.position_manager),
                &POOL_KEYS,
                &[B256::from(prefix)],
            )
            .await?;
        let key = returned
            .get(..POOL_KEY_WORDS * WORD)
            .ok_or(Doubt::Unknown)?;
        if keccak256(key) != id {
            return Err(Doubt::Unknown);
        }

        let word = |index: usize| B256::from_slice(&key[index * WORD..(index + 1) * WORD]);
        let currency = |index: usize| {
            let word = word(index);
            if word[..12].iter().any(|byte| *byte != 0) {
                return Err(Doubt::NotAPool);
            }
            Ok(EthAddress::from_word(word))
        };
        let fee = u32::try_from(U256::from_be_bytes(word(2).0))
            .ok()
            .filter(|fee| *fee <= MAX_FEE && fee & DYNAMIC_FEE_FLAG == 0);

        Ok(PoolInfo::new(
            *candidate.key(),
            SwapFamily::UniswapV4,
            "Uniswap V4".to_owned(),
            vec![currency(0)?, currency(1)?],
            fee,
            Some(manager),
            TagSource::heuristic("pool_registry.uniswap_v4", SOURCE_VERSION),
        ))
    }
}

#[async_trait::async_trait]
impl PoolRegistry for ChainPoolRegistry {
    async fn resolve(&self, wanted: &[PoolCandidate]) -> HashMap<PoolKey, PoolInfo> {
        let mut found: HashMap<PoolKey, PoolInfo> = HashMap::new();
        let mut missing: Vec<PoolCandidate> = Vec::new();
        let mut seen: HashSet<PoolKey> = HashSet::new();

        for candidate in wanted {
            if !seen.insert(*candidate.key()) {
                continue;
            }

            match self.remembered(candidate.key()) {
                Some(Some(info)) => {
                    found.insert(*candidate.key(), info);
                }
                Some(None) => {}
                None => missing.push(*candidate),
            }
        }

        if missing.is_empty() {
            return found;
        }

        for info in self.recall(&missing).await {
            self.remember_pool(info.clone());
            found.insert(*info.key(), info);
        }
        missing.retain(|candidate| !found.contains_key(candidate.key()));

        let verdicts: Vec<(PoolCandidate, Verdict)> = futures::stream::iter(missing)
            .map(|candidate| async move { (candidate, self.verify(candidate).await) })
            .buffer_unordered(ASKING_CONCURRENCY)
            .collect()
            .await;

        let mut fresh: Vec<PoolInfo> = Vec::new();
        for (candidate, verdict) in verdicts {
            match verdict {
                Verdict::Pool(info) => {
                    self.remember_pool(info.clone());
                    found.insert(*candidate.key(), info.clone());
                    fresh.push(info);
                }
                Verdict::Doubted(doubt) => self.remember_doubt(*candidate.key(), doubt),
            }
        }

        self.store(&fresh).await;
        found
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use alloy_primitives::TxHash;
    use domain::eth::{EthReceipt, SourceKind};

    use super::*;

    type Key = (EthAddress, Vec<u8>);

    struct ScriptedRpc {
        answers: Mutex<HashMap<Key, Result<Bytes, String>>>,
        calls: AtomicUsize,
    }

    impl ScriptedRpc {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                answers: Mutex::new(HashMap::new()),
                calls: AtomicUsize::new(0),
            })
        }

        fn say(&self, to: EthAddress, selector: &[u8; 4], arguments: &[B256], word: B256) {
            self.answers.lock().unwrap().insert(
                (to, calldata(selector, arguments)),
                Ok(Bytes::copy_from_slice(word.as_slice())),
            );
        }

        fn say_bytes(
            &self,
            to: EthAddress,
            selector: &[u8; 4],
            arguments: &[B256],
            bytes: Vec<u8>,
        ) {
            self.answers
                .lock()
                .unwrap()
                .insert((to, calldata(selector, arguments)), Ok(Bytes::from(bytes)));
        }

        fn fail(&self, to: EthAddress, selector: &[u8; 4], message: &str) {
            self.answers
                .lock()
                .unwrap()
                .insert((to, calldata(selector, &[])), Err(message.to_owned()));
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl EthRpcSource for ScriptedRpc {
        async fn head_block(&self) -> Result<u64, io::Error> {
            Ok(0)
        }

        async fn call(
            &self,
            to: &EthAddress,
            data: &[u8],
            _block: BlockRef,
        ) -> Result<Bytes, io::Error> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.answers.lock().unwrap().get(&(*to, data.to_vec())) {
                Some(Ok(bytes)) => Ok(bytes.clone()),
                Some(Err(message)) => Err(io::Error::other(message.clone())),
                None => Err(io::Error::other("execution reverted")),
            }
        }

        async fn code(&self, _address: &EthAddress, _block: BlockRef) -> Result<Bytes, io::Error> {
            Ok(Bytes::new())
        }

        async fn receipt(&self, _tx_hash: &TxHash) -> Result<Option<EthReceipt>, io::Error> {
            Ok(None)
        }
    }

    #[derive(Default)]
    struct MemoryPools {
        rows: Mutex<HashMap<PoolKey, PoolInfo>>,
    }

    #[async_trait::async_trait]
    impl PoolRepository for MemoryPools {
        async fn pools(&self, keys: &[PoolKey]) -> Result<Vec<PoolInfo>, io::Error> {
            let rows = self.rows.lock().unwrap();
            Ok(keys
                .iter()
                .filter_map(|key| rows.get(key).cloned())
                .collect())
        }

        async fn remember(&self, pools: &[PoolInfo]) -> Result<(), io::Error> {
            let mut rows = self.rows.lock().unwrap();
            for pool in pools {
                rows.insert(*pool.key(), pool.clone());
            }
            Ok(())
        }
    }

    fn address(last_byte: u8) -> EthAddress {
        EthAddress::from([last_byte; 20])
    }

    fn word(address: EthAddress) -> B256 {
        address.address().into_word()
    }

    fn candidate(address: EthAddress, family: SwapFamily) -> PoolCandidate {
        PoolCandidate::new(PoolKey::new(address, None), family)
    }

    fn uniswap_v2_factory() -> EthAddress {
        EthAddress::from(V2_FACTORIES[0].address)
    }

    fn uniswap_v3_factory() -> EthAddress {
        EthAddress::from(V3_FACTORIES[0].address)
    }

    fn real_v2_pool(rpc: &ScriptedRpc, pool: EthAddress, factory: EthAddress) {
        rpc.say(pool, &FACTORY, &[], word(factory));
        rpc.say(pool, &TOKEN0, &[], word(address(10)));
        rpc.say(pool, &TOKEN1, &[], word(address(11)));
        rpc.say(
            factory,
            &GET_PAIR,
            &[word(address(10)), word(address(11))],
            word(pool),
        );
    }

    fn real_v3_pool(rpc: &ScriptedRpc, pool: EthAddress, factory: EthAddress, fee: u64) {
        rpc.say(pool, &FACTORY, &[], word(factory));
        rpc.say(pool, &TOKEN0, &[], word(address(10)));
        rpc.say(pool, &TOKEN1, &[], word(address(11)));
        rpc.say(pool, &FEE, &[], B256::from(U256::from(fee)));
        rpc.say(
            factory,
            &GET_POOL,
            &[
                word(address(10)),
                word(address(11)),
                B256::from(U256::from(fee)),
            ],
            word(pool),
        );
    }

    #[tokio::test]
    async fn a_pool_its_factory_vouches_for_enters_the_registry() {
        let rpc = ScriptedRpc::new();
        real_v2_pool(&rpc, address(7), uniswap_v2_factory());
        let registry = ChainPoolRegistry::new(rpc, None);

        let resolved = registry
            .resolve(&[candidate(address(7), SwapFamily::UniswapV2)])
            .await;
        let info = &resolved[&PoolKey::new(address(7), None)];

        assert_eq!(info.protocol(), "Uniswap V2");
        assert_eq!(info.tokens(), &[address(10), address(11)]);
        assert_eq!(info.fee(), Some(3000));
        assert_eq!(info.factory(), Some(&uniswap_v2_factory()));
        assert_eq!(info.source().kind(), SourceKind::Heuristic);
        assert_eq!(info.source().name(), "pool_registry.uniswap_v2");
    }

    #[tokio::test]
    async fn a_pool_vouched_for_by_an_unlisted_factory_is_refused() {
        let rpc = ScriptedRpc::new();
        real_v2_pool(&rpc, address(7), address(99));
        let registry = ChainPoolRegistry::new(rpc, None);

        assert!(
            registry
                .resolve(&[candidate(address(7), SwapFamily::UniswapV2)])
                .await
                .is_empty()
        );
    }

    #[tokio::test]
    async fn a_pool_of_an_unlisted_v3_factory_is_refused() {
        let rpc = ScriptedRpc::new();
        real_v3_pool(&rpc, address(7), address(99), 500);
        let registry = ChainPoolRegistry::new(rpc, None);

        assert!(
            registry
                .resolve(&[candidate(address(7), SwapFamily::UniswapV3)])
                .await
                .is_empty()
        );
    }

    #[tokio::test]
    async fn a_contract_that_only_emits_a_swap_event_is_no_pool() {
        let rpc = ScriptedRpc::new();
        let impostor = address(8);
        rpc.say(impostor, &FACTORY, &[], word(uniswap_v2_factory()));
        rpc.say(impostor, &TOKEN0, &[], word(address(10)));
        rpc.say(impostor, &TOKEN1, &[], word(address(11)));
        rpc.say(
            uniswap_v2_factory(),
            &GET_PAIR,
            &[word(address(10)), word(address(11))],
            word(address(7)),
        );
        let registry = ChainPoolRegistry::new(rpc.clone(), None);

        let wanted = [candidate(impostor, SwapFamily::UniswapV2)];
        assert!(registry.resolve(&wanted).await.is_empty());

        let before = rpc.calls();
        assert!(registry.resolve(&wanted).await.is_empty());
        assert_eq!(rpc.calls(), before);
    }

    #[tokio::test]
    async fn a_node_that_fails_leaves_the_question_open() {
        let rpc = ScriptedRpc::new();
        rpc.fail(address(7), &FACTORY, "connection reset");
        let registry =
            ChainPoolRegistry::new(rpc.clone(), None).with_backoff(Duration::ZERO, Duration::ZERO);
        let wanted = [candidate(address(7), SwapFamily::UniswapV2)];

        assert!(registry.resolve(&wanted).await.is_empty());

        real_v2_pool(&rpc, address(7), uniswap_v2_factory());
        assert_eq!(registry.resolve(&wanted).await.len(), 1);
    }

    #[tokio::test]
    async fn a_failed_question_is_not_repeated_within_the_backoff() {
        let rpc = ScriptedRpc::new();
        rpc.fail(address(7), &FACTORY, "connection reset");
        let registry = ChainPoolRegistry::new(rpc.clone(), None);
        let wanted = [candidate(address(7), SwapFamily::UniswapV2)];

        assert!(registry.resolve(&wanted).await.is_empty());
        let before = rpc.calls();
        assert!(registry.resolve(&wanted).await.is_empty());

        assert_eq!(rpc.calls(), before);
    }

    #[tokio::test]
    async fn a_verified_pool_is_never_downgraded_by_a_late_doubt() {
        let rpc = ScriptedRpc::new();
        real_v2_pool(&rpc, address(7), uniswap_v2_factory());
        let registry = ChainPoolRegistry::new(rpc, None);
        let wanted = [candidate(address(7), SwapFamily::UniswapV2)];

        assert_eq!(registry.resolve(&wanted).await.len(), 1);
        registry.remember_doubt(*wanted[0].key(), Doubt::NotAPool);

        assert_eq!(registry.resolve(&wanted).await.len(), 1);
    }

    #[tokio::test]
    async fn a_v3_pool_is_checked_by_its_fee_tier() {
        let rpc = ScriptedRpc::new();
        real_v3_pool(&rpc, address(7), uniswap_v3_factory(), 500);
        let registry = ChainPoolRegistry::new(rpc, None);

        let resolved = registry
            .resolve(&[candidate(address(7), SwapFamily::UniswapV3)])
            .await;
        let info = &resolved[&PoolKey::new(address(7), None)];

        assert_eq!(info.protocol(), "Uniswap V3");
        assert_eq!(info.fee(), Some(500));
        assert_eq!(info.family(), SwapFamily::UniswapV3);
    }

    #[tokio::test]
    async fn a_v3_pool_the_factory_does_not_know_by_that_fee_is_no_pool() {
        let rpc = ScriptedRpc::new();
        real_v3_pool(&rpc, address(7), uniswap_v3_factory(), 500);
        rpc.say(address(7), &FEE, &[], B256::from(U256::from(10_000u64)));
        let registry = ChainPoolRegistry::new(rpc, None);

        assert!(
            registry
                .resolve(&[candidate(address(7), SwapFamily::UniswapV3)])
                .await
                .is_empty()
        );
    }

    fn manager() -> EthAddress {
        EthAddress::from(V4_MANAGERS[0].address)
    }

    fn pool_key(currency1: EthAddress, fee: u64) -> (B256, Vec<u8>) {
        let mut key = Vec::with_capacity(POOL_KEY_WORDS * WORD);
        key.extend_from_slice(word(EthAddress::ZERO).as_slice());
        key.extend_from_slice(word(currency1).as_slice());
        key.extend_from_slice(&U256::from(fee).to_be_bytes::<32>());
        key.extend_from_slice(&U256::from(60u64).to_be_bytes::<32>());
        key.extend_from_slice(word(EthAddress::ZERO).as_slice());
        (keccak256(&key), key)
    }

    fn teach_position_manager(rpc: &ScriptedRpc, id: B256, key: Vec<u8>) {
        let mut prefix = [0u8; WORD];
        prefix[..POOL_ID_PREFIX].copy_from_slice(&id[..POOL_ID_PREFIX]);
        rpc.say_bytes(
            EthAddress::from(V4_MANAGERS[0].position_manager),
            &POOL_KEYS,
            &[B256::from(prefix)],
            key,
        );
    }

    #[tokio::test]
    async fn a_v4_pool_is_read_from_the_position_manager() {
        let rpc = ScriptedRpc::new();
        let (id, key_bytes) = pool_key(address(11), 3000);
        teach_position_manager(&rpc, id, key_bytes);
        let registry = ChainPoolRegistry::new(rpc, None);
        let key = PoolKey::new(manager(), Some(id));

        let resolved = registry
            .resolve(&[PoolCandidate::new(key, SwapFamily::UniswapV4)])
            .await;
        let info = &resolved[&key];

        assert_eq!(info.protocol(), "Uniswap V4");
        assert_eq!(info.tokens(), &[EthAddress::ZERO, address(11)]);
        assert_eq!(info.fee(), Some(3000));
        assert_eq!(info.factory(), Some(&manager()));
    }

    #[tokio::test]
    async fn a_dynamic_fee_is_not_a_number() {
        let rpc = ScriptedRpc::new();
        let (id, key_bytes) = pool_key(address(11), 0x80_0000);
        teach_position_manager(&rpc, id, key_bytes);
        let registry = ChainPoolRegistry::new(rpc, None);
        let key = PoolKey::new(manager(), Some(id));

        let resolved = registry
            .resolve(&[PoolCandidate::new(key, SwapFamily::UniswapV4)])
            .await;

        assert_eq!(resolved[&key].fee(), None);
    }

    #[tokio::test]
    async fn a_pool_key_that_does_not_hash_to_the_id_is_refused() {
        let rpc = ScriptedRpc::new();
        let (_, key_bytes) = pool_key(address(11), 3000);
        let id = B256::with_last_byte(0xaa);
        teach_position_manager(&rpc, id, key_bytes);
        let registry = ChainPoolRegistry::new(rpc, None);
        let key = PoolKey::new(manager(), Some(id));

        assert!(
            registry
                .resolve(&[PoolCandidate::new(key, SwapFamily::UniswapV4)])
                .await
                .is_empty()
        );
    }

    #[tokio::test]
    async fn a_v4_event_from_an_unknown_manager_is_ignored() {
        let rpc = ScriptedRpc::new();
        let registry = ChainPoolRegistry::new(rpc.clone(), None);
        let key = PoolKey::new(address(5), Some(B256::with_last_byte(1)));

        assert!(
            registry
                .resolve(&[PoolCandidate::new(key, SwapFamily::UniswapV4)])
                .await
                .is_empty()
        );
        assert_eq!(rpc.calls(), 0);
    }

    #[tokio::test]
    async fn a_v4_pool_the_position_manager_does_not_know_yet_stays_open() {
        let rpc = ScriptedRpc::new();
        let registry =
            ChainPoolRegistry::new(rpc.clone(), None).with_backoff(Duration::ZERO, Duration::ZERO);
        let (id, key_bytes) = pool_key(address(11), 100);
        let key = PoolKey::new(manager(), Some(id));
        let wanted = [PoolCandidate::new(key, SwapFamily::UniswapV4)];

        assert!(registry.resolve(&wanted).await.is_empty());

        teach_position_manager(&rpc, id, key_bytes);
        assert_eq!(registry.resolve(&wanted).await.len(), 1);
    }

    #[tokio::test]
    async fn a_verified_pool_survives_a_restart() {
        let rpc = ScriptedRpc::new();
        real_v2_pool(&rpc, address(7), uniswap_v2_factory());
        let storage = Arc::new(MemoryPools::default());
        let wanted = [candidate(address(7), SwapFamily::UniswapV2)];

        let first = ChainPoolRegistry::new(rpc, Some(storage.clone()));
        assert_eq!(first.resolve(&wanted).await.len(), 1);

        let mute = ScriptedRpc::new();
        let restarted = ChainPoolRegistry::new(mute.clone(), Some(storage));
        let resolved = restarted.resolve(&wanted).await;

        assert_eq!(resolved.len(), 1);
        assert_eq!(mute.calls(), 0);
    }
}
