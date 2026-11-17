use std::{io, net::IpAddr, str::FromStr};

use application::observe::{LinkContribution, ObservationStore, ObservedBroadcast};
use domain::{
    account::IpLinkId,
    eth::EthAddress,
    observe::{IpWalletLink, TlsObservation},
};
use sqlx::{PgPool, Row, postgres::PgRow};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct PgObservations {
    pool: PgPool,
}

impl PgObservations {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn failed(error: sqlx::Error) -> io::Error {
    io::Error::other(format!("postgres: {error}"))
}

fn corrupt(what: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("postgres holds a broken {what}"),
    )
}

fn chain_param(chain_id: u64) -> Result<i64, io::Error> {
    i64::try_from(chain_id)
        .map_err(|_| io::Error::other(format!("chain {chain_id} does not fit postgres")))
}

fn chain_of(row: &PgRow, column: &str) -> Result<u64, io::Error> {
    let raw: i64 = row.try_get(column).map_err(failed)?;
    u64::try_from(raw).map_err(|_| corrupt("chain id"))
}

fn moment(seconds: u64) -> Result<OffsetDateTime, io::Error> {
    OffsetDateTime::from_unix_timestamp(seconds as i64).map_err(|_| corrupt("timestamp"))
}

fn seconds_of(row: &PgRow, column: &str) -> Result<u64, io::Error> {
    let moment: OffsetDateTime = row.try_get(column).map_err(failed)?;
    u64::try_from(moment.unix_timestamp()).map_err(|_| corrupt("timestamp"))
}

fn ip_of(row: &PgRow, column: &str) -> Result<IpAddr, io::Error> {
    let raw: String = row.try_get(column).map_err(failed)?;
    IpAddr::from_str(&raw).map_err(|_| corrupt("ip address"))
}

fn address_of(row: &PgRow, column: &str) -> Result<EthAddress, io::Error> {
    let raw: Vec<u8> = row.try_get(column).map_err(failed)?;
    EthAddress::try_from(raw).map_err(|_| corrupt("link address"))
}

#[async_trait::async_trait]
impl ObservationStore for PgObservations {
    async fn insert(&self, observation: &TlsObservation) -> Result<(), io::Error> {
        sqlx::query(
            "insert into tls_observations (id, src_ip, chain_id, intent, confidence, observed_at) \
             values ($1, $2, $3, $4, $5, $6)",
        )
        .bind(observation.id.uuid())
        .bind(observation.src_ip.to_string())
        .bind(chain_param(observation.chain_id)?)
        .bind(observation.intent.as_str())
        .bind(observation.confidence)
        .bind(moment(observation.observed_at)?)
        .execute(&self.pool)
        .await
        .map_err(failed)?;
        Ok(())
    }

    async fn broadcasts_between(
        &self,
        chain_id: u64,
        from: u64,
        to: u64,
    ) -> Result<Vec<ObservedBroadcast>, io::Error> {
        let rows = sqlx::query(
            "select src_ip, observed_at, confidence from tls_observations \
             where chain_id = $1 and intent = 'broadcast' and observed_at >= $2 and observed_at <= $3",
        )
        .bind(chain_param(chain_id)?)
        .bind(moment(from)?)
        .bind(moment(to)?)
        .fetch_all(&self.pool)
        .await
        .map_err(failed)?;

        rows.iter()
            .map(|row| {
                Ok(ObservedBroadcast {
                    src_ip: ip_of(row, "src_ip")?,
                    observed_at: seconds_of(row, "observed_at")?,
                    confidence: row.try_get("confidence").map_err(failed)?,
                })
            })
            .collect()
    }

    async fn apply_link(&self, contribution: &LinkContribution) -> Result<(), io::Error> {
        sqlx::query(
            "insert into ip_wallet_links \
             (id, src_ip, chain_id, address, hits, address_txs, confidence, first_seen, last_seen) \
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             on conflict (src_ip, chain_id, address) do update set \
             confidence = ip_wallet_links.confidence \
                 + (1 - ip_wallet_links.confidence) * excluded.confidence, \
             hits = ip_wallet_links.hits + excluded.hits, \
             address_txs = ip_wallet_links.address_txs + excluded.address_txs, \
             first_seen = least(ip_wallet_links.first_seen, excluded.first_seen), \
             last_seen = greatest(ip_wallet_links.last_seen, excluded.last_seen)",
        )
        .bind(IpLinkId::new().uuid())
        .bind(contribution.src_ip.to_string())
        .bind(chain_param(contribution.chain_id)?)
        .bind(contribution.address.address().as_slice().to_vec())
        .bind(i32::try_from(contribution.hits).unwrap_or(i32::MAX))
        .bind(i32::try_from(contribution.address_txs).unwrap_or(i32::MAX))
        .bind(contribution.contribution)
        .bind(moment(contribution.first_seen)?)
        .bind(moment(contribution.last_seen)?)
        .execute(&self.pool)
        .await
        .map_err(failed)?;
        Ok(())
    }

    async fn links(&self, chain_id: u64) -> Result<Vec<IpWalletLink>, io::Error> {
        let rows = sqlx::query(
            "select id, src_ip, chain_id, address, hits, address_txs, confidence, first_seen, last_seen \
             from ip_wallet_links where chain_id = $1 \
             order by (hits::float8 / nullif(address_txs, 0)) desc nulls last, hits desc",
        )
        .bind(chain_param(chain_id)?)
        .fetch_all(&self.pool)
        .await
        .map_err(failed)?;

        rows.iter()
            .map(|row| {
                let id: Uuid = row.try_get("id").map_err(failed)?;
                let hits: i32 = row.try_get("hits").map_err(failed)?;
                let address_txs: i32 = row.try_get("address_txs").map_err(failed)?;
                Ok(IpWalletLink {
                    id: IpLinkId::from_uuid(id),
                    src_ip: ip_of(row, "src_ip")?,
                    chain_id: chain_of(row, "chain_id")?,
                    address: address_of(row, "address")?,
                    hits: u32::try_from(hits).map_err(|_| corrupt("link hits"))?,
                    address_txs: u32::try_from(address_txs)
                        .map_err(|_| corrupt("link address txs"))?,
                    confidence: row.try_get("confidence").map_err(failed)?,
                    first_seen: seconds_of(row, "first_seen")?,
                    last_seen: seconds_of(row, "last_seen")?,
                })
            })
            .collect()
    }

    async fn watermark(&self, chain_id: u64) -> Result<Option<u64>, io::Error> {
        let row =
            sqlx::query("select watermark_block from tls_correlation_state where chain_id = $1")
                .bind(chain_param(chain_id)?)
                .fetch_optional(&self.pool)
                .await
                .map_err(failed)?;

        match row {
            Some(row) => {
                let block: i64 = row.try_get("watermark_block").map_err(failed)?;
                Ok(Some(
                    u64::try_from(block).map_err(|_| corrupt("watermark"))?,
                ))
            }
            None => Ok(None),
        }
    }

    async fn set_watermark(&self, chain_id: u64, block: u64) -> Result<(), io::Error> {
        sqlx::query(
            "insert into tls_correlation_state (chain_id, watermark_block, updated_at) \
             values ($1, $2, now()) \
             on conflict (chain_id) do update set \
             watermark_block = excluded.watermark_block, updated_at = now()",
        )
        .bind(chain_param(chain_id)?)
        .bind(i64::try_from(block).map_err(|_| corrupt("watermark"))?)
        .execute(&self.pool)
        .await
        .map_err(failed)?;
        Ok(())
    }

    async fn reset(&self, chain_id: u64) -> Result<(), io::Error> {
        let chain = chain_param(chain_id)?;
        sqlx::query("delete from ip_wallet_links where chain_id = $1")
            .bind(chain)
            .execute(&self.pool)
            .await
            .map_err(failed)?;
        sqlx::query("delete from tls_correlation_state where chain_id = $1")
            .bind(chain)
            .execute(&self.pool)
            .await
            .map_err(failed)?;
        Ok(())
    }
}
