use std::{
    collections::HashMap,
    io,
    sync::{Arc, RwLock},
};

use alloy_primitives::{Bytes, U256};
use futures::StreamExt;

use domain::eth::{Actor, ActorHint, ActorKind, BlockRef, ContractKind, EthAddress};

use crate::eth::ports::{ActorRepository, ActorResolver, EthRpcSource};

const SYMBOL_SELECTOR: [u8; 4] = [0x95, 0xd8, 0x9b, 0x41];

const DECIMALS_SELECTOR: [u8; 4] = [0x31, 0x3c, 0xe5, 0x67];

const DEFAULT_DECIMALS: u8 = 18;

const ASKING_CONCURRENCY: usize = 20;

pub struct CachingActorResolver {
    rpc: Arc<dyn EthRpcSource>,
    repository: Option<Arc<dyn ActorRepository>>,
    known: RwLock<HashMap<EthAddress, ActorKind>>,
}

impl CachingActorResolver {
    pub fn new(rpc: Arc<dyn EthRpcSource>, repository: Option<Arc<dyn ActorRepository>>) -> Self {
        Self {
            rpc,
            repository,
            known: RwLock::new(HashMap::new()),
        }
    }

    fn remembered(&self, address: &EthAddress) -> Option<ActorKind> {
        self.known
            .read()
            .expect("actor cache poisoned")
            .get(address)
            .cloned()
    }

    fn remember(&self, actors: &[Actor]) {
        let mut known = self.known.write().expect("actor cache poisoned");

        for actor in actors {
            known.insert(*actor.address(), actor.kind().clone());
        }
    }

    async fn recall(&self, wanted: &[EthAddress]) -> Vec<Actor> {
        let Some(repository) = self.repository.as_ref() else {
            return Vec::new();
        };

        if wanted.is_empty() {
            return Vec::new();
        }

        match repository.actors(wanted).await {
            Ok(actors) => actors,
            Err(error) => {
                tracing::warn!("clickhouse forgot what it knew about actors: {error}");
                Vec::new()
            }
        }
    }

    async fn store(&self, actors: &[Actor]) {
        let Some(repository) = self.repository.as_ref() else {
            return;
        };

        if actors.is_empty() {
            return;
        }

        if let Err(error) = repository.remember(actors).await {
            tracing::warn!("could not write {} actors down: {error}", actors.len());
        }
    }

    async fn ask_chain(&self, address: EthAddress, hint: Option<ActorHint>) -> Actor {
        let kind = match hint {
            Some(ActorHint::Erc20) => ActorKind::Contract(self.ask_token(&address).await),
            _ => match self.rpc.code(&address, BlockRef::Latest).await {
                Ok(code) if code.is_empty() => ActorKind::Eoa,
                Ok(_) => ActorKind::Contract(ContractKind::Plain),
                Err(error) => {
                    tracing::warn!("eth_getCode on {address} failed, leaving it unknown: {error}");
                    ActorKind::Unknown
                }
            },
        };

        Actor::new(address, kind)
    }

    async fn ask_token(&self, token: &EthAddress) -> ContractKind {
        let (symbol, decimals) = tokio::join!(self.ask_symbol(token), self.ask_decimals(token));

        ContractKind::Erc20 { symbol, decimals }
    }

    async fn ask_symbol(&self, token: &EthAddress) -> String {
        match self
            .rpc
            .call(token, &SYMBOL_SELECTOR, BlockRef::Latest)
            .await
        {
            Ok(returned) => decode_symbol(&returned).unwrap_or_else(|| token.to_string()),
            Err(error) if answered_with_revert(&error) => {
                tracing::warn!("{token} has no symbol(), naming it by address from now on");
                token.to_string()
            }
            Err(error) => {
                tracing::warn!("symbol() on {token} failed, naming it by address: {error}");
                token.to_string()
            }
        }
    }

    async fn ask_decimals(&self, token: &EthAddress) -> u8 {
        match self
            .rpc
            .call(token, &DECIMALS_SELECTOR, BlockRef::Latest)
            .await
        {
            Ok(returned) => decode_decimals(&returned).unwrap_or(DEFAULT_DECIMALS),
            Err(error) if answered_with_revert(&error) => {
                tracing::warn!(
                    "{token} has no decimals(), assuming {DEFAULT_DECIMALS} from now on"
                );
                DEFAULT_DECIMALS
            }
            Err(error) => {
                tracing::warn!(
                    "decimals() on {token} failed, assuming {DEFAULT_DECIMALS}: {error}"
                );
                DEFAULT_DECIMALS
            }
        }
    }
}

fn settled(hint: Option<ActorHint>) -> Option<ActorKind> {
    match hint {
        Some(ActorHint::Eoa) => Some(ActorKind::Eoa),
        Some(ActorHint::Contract) => Some(ActorKind::Contract(ContractKind::Plain)),
        Some(ActorHint::Erc20) | None => None,
    }
}

fn worth_keeping(kind: &ActorKind, hint: Option<ActorHint>) -> bool {
    match kind {
        ActorKind::Unknown => false,
        ActorKind::Eoa | ActorKind::Contract(ContractKind::Plain) => hint != Some(ActorHint::Erc20),
        ActorKind::Contract(ContractKind::Erc20 { .. }) => true,
    }
}

#[async_trait::async_trait]
impl ActorResolver for CachingActorResolver {
    async fn resolve(
        &self,
        wanted: HashMap<EthAddress, Option<ActorHint>>,
    ) -> HashMap<EthAddress, Actor> {
        let mut resolved: HashMap<EthAddress, Actor> = HashMap::with_capacity(wanted.len());
        let mut asking: Vec<EthAddress> = Vec::new();

        for (address, hint) in &wanted {
            match self.remembered(address) {
                Some(kind) if worth_keeping(&kind, *hint) => {
                    resolved.insert(*address, Actor::new(*address, kind));
                }
                _ => match settled(*hint) {
                    Some(kind) => {
                        resolved.insert(*address, Actor::new(*address, kind));
                    }
                    None => asking.push(*address),
                },
            }
        }

        for actor in self.recall(&asking).await {
            if worth_keeping(actor.kind(), wanted.get(actor.address()).copied().flatten()) {
                resolved.insert(*actor.address(), actor);
            }
        }

        let missing: Vec<EthAddress> = asking
            .into_iter()
            .filter(|address| !resolved.contains_key(address))
            .collect();

        let asked: Vec<Actor> = futures::stream::iter(missing)
            .map(|address| {
                let hint = wanted.get(&address).copied().flatten();
                async move { self.ask_chain(address, hint).await }
            })
            .buffer_unordered(ASKING_CONCURRENCY)
            .collect()
            .await;

        let fresh: Vec<Actor> = asked
            .iter()
            .filter(|actor| !matches!(actor.kind(), ActorKind::Unknown))
            .cloned()
            .collect();

        self.remember(&fresh);
        self.store(&fresh).await;

        for actor in asked {
            resolved.insert(*actor.address(), actor);
        }

        resolved
    }
}

fn answered_with_revert(error: &io::Error) -> bool {
    error.to_string().contains("execution reverted")
}

fn decode_symbol(returned: &Bytes) -> Option<String> {
    if returned.len() == 32 {
        let text: Vec<u8> = returned
            .iter()
            .take_while(|byte| **byte != 0)
            .copied()
            .collect();
        return String::from_utf8(text).ok().filter(|name| !name.is_empty());
    }

    let word_at = |at: usize| -> Option<usize> {
        let word = returned.get(at..at.checked_add(32)?)?;
        usize::try_from(U256::from_be_slice(word)).ok()
    };

    let offset = word_at(0)?;
    let length = word_at(offset)?;
    let start = offset.checked_add(32)?;
    let text = returned.get(start..start.checked_add(length)?)?;

    String::from_utf8(text.to_vec())
        .ok()
        .filter(|name| !name.is_empty())
}

fn decode_decimals(returned: &Bytes) -> Option<u8> {
    let word = returned.get(..32)?;
    u8::try_from(U256::from_be_slice(word))
        .ok()
        .filter(|d| *d <= 36)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use alloy_primitives::TxHash;
    use domain::eth::EthReceipt;

    use super::*;

    struct CountingRpc {
        code: Bytes,
        calls: Mutex<Vec<EthAddress>>,
    }

    impl CountingRpc {
        fn new(code: Bytes) -> Self {
            Self {
                code,
                calls: Mutex::new(Vec::new()),
            }
        }

        fn asked(&self) -> Vec<EthAddress> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl EthRpcSource for CountingRpc {
        async fn head_block(&self) -> Result<u64, io::Error> {
            Ok(21_000_000)
        }

        async fn call(
            &self,
            to: &EthAddress,
            data: &[u8],
            _block: BlockRef,
        ) -> Result<Bytes, io::Error> {
            self.calls.lock().unwrap().push(*to);

            let mut word = [0u8; 32];

            if data == SYMBOL_SELECTOR.as_slice() {
                word[..4].copy_from_slice(b"USDC");
            } else if data == DECIMALS_SELECTOR.as_slice() {
                word[31] = 6;
            } else {
                return Err(io::Error::other("execution reverted"));
            }

            Ok(Bytes::from(word.to_vec()))
        }

        async fn code(&self, address: &EthAddress, _block: BlockRef) -> Result<Bytes, io::Error> {
            self.calls.lock().unwrap().push(*address);

            Ok(self.code.clone())
        }

        async fn receipt(&self, _tx_hash: &TxHash) -> Result<Option<EthReceipt>, io::Error> {
            Ok(None)
        }
    }

    #[derive(Default)]
    struct MemoryActors {
        rows: Mutex<Vec<Actor>>,
    }

    #[async_trait::async_trait]
    impl ActorRepository for MemoryActors {
        async fn actors(&self, addresses: &[EthAddress]) -> Result<Vec<Actor>, io::Error> {
            let rows = self.rows.lock().unwrap();

            Ok(rows
                .iter()
                .filter(|actor| addresses.contains(actor.address()))
                .cloned()
                .collect())
        }

        async fn remember(&self, actors: &[Actor]) -> Result<(), io::Error> {
            self.rows.lock().unwrap().extend_from_slice(actors);

            Ok(())
        }
    }

    fn address(last_byte: u8) -> EthAddress {
        EthAddress::from([last_byte; 20])
    }

    fn resolver(rpc: Arc<CountingRpc>) -> CachingActorResolver {
        CachingActorResolver::new(rpc, None)
    }

    fn hinted(address: EthAddress, hint: ActorHint) -> HashMap<EthAddress, Option<ActorHint>> {
        HashMap::from([(address, Some(hint))])
    }

    #[tokio::test]
    async fn a_hinted_wallet_costs_nothing() {
        let rpc = Arc::new(CountingRpc::new(Bytes::new()));
        let resolved = resolver(rpc.clone())
            .resolve(hinted(address(1), ActorHint::Eoa))
            .await;

        assert_eq!(resolved[&address(1)].kind(), &ActorKind::Eoa);
        assert!(rpc.asked().is_empty());
    }

    #[tokio::test]
    async fn a_hinted_contract_costs_nothing() {
        let rpc = Arc::new(CountingRpc::new(Bytes::new()));
        let resolved = resolver(rpc.clone())
            .resolve(hinted(address(1), ActorHint::Contract))
            .await;

        assert!(resolved[&address(1)].is_contract());
        assert!(rpc.asked().is_empty());
    }

    #[tokio::test]
    async fn a_token_is_asked_for_its_name() {
        let rpc = Arc::new(CountingRpc::new(Bytes::new()));
        let resolved = resolver(rpc.clone())
            .resolve(hinted(address(9), ActorHint::Erc20))
            .await;

        assert_eq!(resolved[&address(9)].erc20(), Some(("USDC", 6)));
    }

    #[tokio::test]
    async fn a_token_is_asked_only_once() {
        let rpc = Arc::new(CountingRpc::new(Bytes::new()));
        let resolver = resolver(rpc.clone());

        resolver.resolve(hinted(address(9), ActorHint::Erc20)).await;
        let before = rpc.asked().len();
        let again = resolver.resolve(hinted(address(9), ActorHint::Erc20)).await;

        assert_eq!(again[&address(9)].erc20(), Some(("USDC", 6)));
        assert_eq!(rpc.asked().len(), before);
    }

    #[tokio::test]
    async fn an_unhinted_address_without_code_is_a_wallet() {
        let rpc = Arc::new(CountingRpc::new(Bytes::new()));
        let resolved = resolver(rpc.clone())
            .resolve(HashMap::from([(address(5), None)]))
            .await;

        assert_eq!(resolved[&address(5)].kind(), &ActorKind::Eoa);
        assert_eq!(rpc.asked(), vec![address(5)]);
    }

    #[tokio::test]
    async fn an_unhinted_address_with_code_is_a_contract() {
        let rpc = Arc::new(CountingRpc::new(Bytes::from_static(&[0x60, 0x80])));
        let resolved = resolver(rpc)
            .resolve(HashMap::from([(address(5), None)]))
            .await;

        assert!(resolved[&address(5)].is_contract());
        assert_eq!(resolved[&address(5)].erc20(), None);
    }

    #[tokio::test]
    async fn a_wallet_remembered_from_before_never_satisfies_a_token() {
        let rpc = Arc::new(CountingRpc::new(Bytes::new()));
        let resolver = resolver(rpc.clone());

        resolver.resolve(HashMap::from([(address(9), None)])).await;
        let resolved = resolver.resolve(hinted(address(9), ActorHint::Erc20)).await;

        assert_eq!(resolved[&address(9)].erc20(), Some(("USDC", 6)));
    }

    #[tokio::test]
    async fn a_rediscovered_token_comes_from_storage_not_the_chain() {
        let rpc = Arc::new(CountingRpc::new(Bytes::new()));
        let storage = Arc::new(MemoryActors::default());

        let first = CachingActorResolver::new(rpc.clone(), Some(storage.clone()));
        first.resolve(hinted(address(9), ActorHint::Erc20)).await;
        let after_first = rpc.asked().len();

        let restarted = CachingActorResolver::new(rpc.clone(), Some(storage));
        let resolved = restarted
            .resolve(hinted(address(9), ActorHint::Erc20))
            .await;

        assert_eq!(resolved[&address(9)].erc20(), Some(("USDC", 6)));
        assert_eq!(rpc.asked().len(), after_first);
    }

    #[tokio::test]
    async fn a_silent_node_leaves_the_actor_unknown() {
        struct MuteRpc;

        #[async_trait::async_trait]
        impl EthRpcSource for MuteRpc {
            async fn head_block(&self) -> Result<u64, io::Error> {
                Ok(0)
            }

            async fn call(
                &self,
                _to: &EthAddress,
                _data: &[u8],
                _block: BlockRef,
            ) -> Result<Bytes, io::Error> {
                Err(io::Error::other("node is down"))
            }

            async fn code(
                &self,
                _address: &EthAddress,
                _block: BlockRef,
            ) -> Result<Bytes, io::Error> {
                Err(io::Error::other("node is down"))
            }

            async fn receipt(&self, _tx_hash: &TxHash) -> Result<Option<EthReceipt>, io::Error> {
                Ok(None)
            }
        }

        let storage = Arc::new(MemoryActors::default());
        let resolver = CachingActorResolver::new(Arc::new(MuteRpc), Some(storage.clone()));
        let resolved = resolver.resolve(HashMap::from([(address(7), None)])).await;

        assert_eq!(resolved[&address(7)].kind(), &ActorKind::Unknown);
        assert!(storage.rows.lock().unwrap().is_empty());
    }
}
