use std::io;

use application::eth::ports::EthTxCache;
use redis::{AsyncCommands, aio::ConnectionManager};

const INDEXED_KEY: &str = "ledgerscope:eth:indexed_blocks";

pub struct RedisTxCache {
    connection: ConnectionManager,
    key: String,
}

impl RedisTxCache {
    pub async fn connect(url: &str) -> Result<Self, io::Error> {
        Self::connect_with_key(url, INDEXED_KEY).await
    }

    pub async fn connect_with_key(url: &str, key: &str) -> Result<Self, io::Error> {
        let client = redis::Client::open(url)
            .map_err(|error| io::Error::other(format!("redis url is unusable: {error}")))?;

        let connection = ConnectionManager::new(client)
            .await
            .map_err(|error| io::Error::other(format!("redis is unreachable: {error}")))?;

        Ok(Self {
            connection,
            key: key.to_owned(),
        })
    }
}

#[async_trait::async_trait]
impl EthTxCache for RedisTxCache {
    async fn indexed_blocks(
        &self,
        lower_block: u64,
        highest_block: u64,
    ) -> Result<Vec<u64>, io::Error> {
        let mut connection = self.connection.clone();

        let blocks: Vec<u64> = connection
            .zrangebyscore(&self.key, lower_block as f64, highest_block as f64)
            .await
            .map_err(|error| io::Error::other(format!("redis lookup failed: {error}")))?;

        Ok(blocks)
    }

    async fn remember_indexed(&self, blocks: &[u64]) -> Result<(), io::Error> {
        if blocks.is_empty() {
            return Ok(());
        }

        let mut connection = self.connection.clone();
        let members: Vec<(f64, u64)> = blocks.iter().map(|&block| (block as f64, block)).collect();

        connection
            .zadd_multiple::<_, _, _, ()>(&self.key, &members)
            .await
            .map_err(|error| io::Error::other(format!("redis write failed: {error}")))?;

        Ok(())
    }
}
