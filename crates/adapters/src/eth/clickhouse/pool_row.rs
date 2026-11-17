use std::{io, str::FromStr};

use alloy_primitives::B256;
use serde::{Deserialize, Serialize};

use domain::eth::{EthAddress, PoolInfo, PoolKey, SourceKind, SwapFamily, TagSource};

#[derive(Debug, Serialize, Deserialize)]
pub struct PoolRow {
    pub address: String,
    pub pool_id: String,
    pub family: String,
    pub protocol: String,
    pub tokens: Vec<String>,
    pub fee: Option<u32>,
    pub factory: Option<String>,
    pub source_kind: String,
    pub source_name: String,
    pub source_version: u32,
}

impl PoolRow {
    pub fn from_pool(pool: &PoolInfo) -> Self {
        Self {
            address: pool.address().to_string(),
            pool_id: pool
                .key()
                .pool_id()
                .map(|id| id.to_string())
                .unwrap_or_default(),
            family: pool.family().as_str().to_owned(),
            protocol: pool.protocol().to_owned(),
            tokens: pool.tokens().iter().map(EthAddress::to_string).collect(),
            fee: pool.fee(),
            factory: pool.factory().map(EthAddress::to_string),
            source_kind: pool.source().kind().as_str().to_owned(),
            source_name: pool.source().name().to_owned(),
            source_version: pool.source().version(),
        }
    }

    pub fn into_pool(self) -> Result<PoolInfo, io::Error> {
        let broken = |what: &str| {
            io::Error::other(format!(
                "clickhouse holds a broken pool {}: {what}",
                self.address
            ))
        };

        let address = EthAddress::from_str(&self.address).map_err(|_| broken("address"))?;
        let pool_id = if self.pool_id.is_empty() {
            None
        } else {
            Some(B256::from_str(&self.pool_id).map_err(|_| broken("id"))?)
        };
        let family = SwapFamily::parse(&self.family).ok_or_else(|| broken("family"))?;
        let tokens = self
            .tokens
            .iter()
            .map(|token| EthAddress::from_str(token))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| broken("token"))?;
        let factory = self
            .factory
            .as_deref()
            .map(EthAddress::from_str)
            .transpose()
            .map_err(|_| broken("factory"))?;
        let kind = SourceKind::parse(&self.source_kind).ok_or_else(|| broken("source"))?;

        Ok(PoolInfo::new(
            PoolKey::new(address, pool_id),
            family,
            self.protocol,
            tokens,
            self.fee,
            factory,
            TagSource::new(kind, self.source_name, self.source_version),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(pool_id: Option<B256>) -> PoolInfo {
        PoolInfo::new(
            PoolKey::new(EthAddress::from([7u8; 20]), pool_id),
            SwapFamily::UniswapV3,
            "Uniswap V3".to_owned(),
            vec![EthAddress::from([1u8; 20]), EthAddress::from([2u8; 20])],
            Some(500),
            Some(EthAddress::from([9u8; 20])),
            TagSource::heuristic("pool_registry.uniswap_v3", 1),
        )
    }

    #[test]
    fn a_pool_keeps_everything_through_the_database() {
        let original = pool(None);

        let restored = PoolRow::from_pool(&original).into_pool().unwrap();

        assert_eq!(restored, original);
    }

    #[test]
    fn a_v4_pool_keeps_its_id() {
        let original = pool(Some(B256::with_last_byte(0xaa)));

        let restored = PoolRow::from_pool(&original).into_pool().unwrap();

        assert_eq!(restored, original);
    }

    #[test]
    fn a_family_we_never_wrote_is_refused() {
        let mut row = PoolRow::from_pool(&pool(None));
        row.family = "curve".to_owned();

        assert!(row.into_pool().is_err());
    }

    #[test]
    fn a_pool_without_provenance_is_refused() {
        let mut row = PoolRow::from_pool(&pool(None));
        row.source_kind = String::new();

        assert!(row.into_pool().is_err());
    }
}
