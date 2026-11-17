use alloy_primitives::{B256, U256};

use crate::eth::{EthAddress, TagSource};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SwapFamily {
    UniswapV2,
    UniswapV3,
    UniswapV4,
}

impl SwapFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            SwapFamily::UniswapV2 => "uniswap_v2",
            SwapFamily::UniswapV3 => "uniswap_v3",
            SwapFamily::UniswapV4 => "uniswap_v4",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "uniswap_v2" => Some(SwapFamily::UniswapV2),
            "uniswap_v3" => Some(SwapFamily::UniswapV3),
            "uniswap_v4" => Some(SwapFamily::UniswapV4),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PoolKey {
    address: EthAddress,
    pool_id: Option<B256>,
}

impl PoolKey {
    pub fn new(address: EthAddress, pool_id: Option<B256>) -> Self {
        Self { address, pool_id }
    }

    pub fn address(&self) -> &EthAddress {
        &self.address
    }

    pub fn pool_id(&self) -> Option<&B256> {
        self.pool_id.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PoolCandidate {
    key: PoolKey,
    family: SwapFamily,
}

impl PoolCandidate {
    pub fn new(key: PoolKey, family: SwapFamily) -> Self {
        Self { key, family }
    }

    pub fn key(&self) -> &PoolKey {
        &self.key
    }

    pub fn family(&self) -> SwapFamily {
        self.family
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolInfo {
    key: PoolKey,
    family: SwapFamily,
    protocol: String,
    tokens: Vec<EthAddress>,
    fee: Option<u32>,
    factory: Option<EthAddress>,
    source: TagSource,
}

impl PoolInfo {
    pub fn new(
        key: PoolKey,
        family: SwapFamily,
        protocol: String,
        tokens: Vec<EthAddress>,
        fee: Option<u32>,
        factory: Option<EthAddress>,
        source: TagSource,
    ) -> Self {
        Self {
            key,
            family,
            protocol,
            tokens,
            fee,
            factory,
            source,
        }
    }

    pub fn key(&self) -> &PoolKey {
        &self.key
    }

    pub fn address(&self) -> &EthAddress {
        self.key.address()
    }

    pub fn family(&self) -> SwapFamily {
        self.family
    }

    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    pub fn tokens(&self) -> &[EthAddress] {
        &self.tokens
    }

    pub fn fee(&self) -> Option<u32> {
        self.fee
    }

    pub fn factory(&self) -> Option<&EthAddress> {
        self.factory.as_ref()
    }

    pub fn source(&self) -> &TagSource {
        &self.source
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Swap {
    pool: PoolInfo,
    token_in: EthAddress,
    amount_in: U256,
    token_out: EthAddress,
    amount_out: U256,
}

impl Swap {
    pub fn new(
        pool: PoolInfo,
        token_in: EthAddress,
        amount_in: U256,
        token_out: EthAddress,
        amount_out: U256,
    ) -> Self {
        Self {
            pool,
            token_in,
            amount_in,
            token_out,
            amount_out,
        }
    }

    pub fn pool(&self) -> &PoolInfo {
        &self.pool
    }

    pub fn token_in(&self) -> &EthAddress {
        &self.token_in
    }

    pub fn amount_in(&self) -> U256 {
        self.amount_in
    }

    pub fn token_out(&self) -> &EthAddress {
        &self.token_out
    }

    pub fn amount_out(&self) -> U256 {
        self.amount_out
    }

    pub fn tokens(&self) -> [&EthAddress; 2] {
        [&self.token_in, &self.token_out]
    }
}
