use std::{io, str::FromStr};

use alloy_primitives::{B256, Bytes, TxHash, U256};
use serde::{Deserialize, Serialize};

use domain::eth::{EthAddress, EthLog, EthReceipt, EthTx, MinedTx};

#[derive(Debug, Serialize, Deserialize)]
pub struct TxRow {
    pub tx_hash: String,
    pub block_number: u64,
    pub timestamp: u64,
    pub amount: String,
    pub from_address: String,
    pub to_address: Option<String>,
    pub data: String,
    pub succeeded: u8,
    pub contract_address: Option<String>,
    pub log_addresses: Vec<String>,
    pub log_topics: Vec<Vec<String>>,
    pub log_data: Vec<String>,
}

impl TxRow {
    pub fn from_mined(mined: &MinedTx) -> Self {
        let tx = mined.tx();
        let receipt = mined.receipt();

        Self {
            tx_hash: tx.tx_hash().to_string(),
            block_number: tx.block_number(),
            timestamp: tx.timestamp(),
            amount: format!("{:#x}", tx.amount()),
            from_address: tx.from().to_string(),
            to_address: tx.to().map(EthAddress::to_string),
            data: tx.data().to_string(),
            succeeded: u8::from(receipt.succeeded()),
            contract_address: receipt.contract_address().map(EthAddress::to_string),
            log_addresses: receipt
                .logs()
                .iter()
                .map(|log| log.address().to_string())
                .collect(),
            log_topics: receipt
                .logs()
                .iter()
                .map(|log| log.topics().iter().map(B256::to_string).collect())
                .collect(),
            log_data: receipt
                .logs()
                .iter()
                .map(|log| log.data().to_string())
                .collect(),
        }
    }

    pub fn into_mined(self) -> Result<MinedTx, io::Error> {
        let tx = EthTx::builder()
            .tx_hash(parse::<TxHash>(&self.tx_hash, "tx_hash")?)
            .block_number(self.block_number)
            .timestamp(self.timestamp)
            .amount(parse::<U256>(&self.amount, "amount")?)
            .from(parse::<EthAddress>(&self.from_address, "from_address")?)
            .maybe_to(
                self.to_address
                    .as_deref()
                    .map(|value| parse::<EthAddress>(value, "to_address"))
                    .transpose()?,
            )
            .data(parse::<Bytes>(&self.data, "data")?)
            .build();

        let receipt = EthReceipt::new(
            self.succeeded != 0,
            self.contract_address
                .as_deref()
                .map(|value| parse::<EthAddress>(value, "contract_address"))
                .transpose()?,
            self.logs()?,
        );

        Ok(MinedTx::new(tx, receipt))
    }

    fn logs(&self) -> Result<Vec<EthLog>, io::Error> {
        if self.log_addresses.len() != self.log_topics.len()
            || self.log_addresses.len() != self.log_data.len()
        {
            return Err(io::Error::other(format!(
                "{} has ragged log columns",
                self.tx_hash
            )));
        }

        self.log_addresses
            .iter()
            .zip(&self.log_topics)
            .zip(&self.log_data)
            .map(|((address, topics), data)| {
                Ok(EthLog::new(
                    parse::<EthAddress>(address, "log_addresses")?,
                    topics
                        .iter()
                        .map(|topic| parse::<B256>(topic, "log_topics"))
                        .collect::<Result<_, _>>()?,
                    parse::<Bytes>(data, "log_data")?,
                ))
            })
            .collect()
    }
}

fn parse<T: FromStr>(value: &str, column: &str) -> Result<T, io::Error>
where
    T::Err: std::fmt::Display,
{
    T::from_str(value).map_err(|error| io::Error::other(format!("{column} is unreadable: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MinedTx {
        let tx = EthTx::builder()
            .tx_hash(TxHash::with_last_byte(7))
            .block_number(21_000_000)
            .timestamp(1_700_000_000)
            .amount(U256::from(123_456_789_u64) * U256::from(10_u64).pow(U256::from(18)))
            .from(EthAddress::from([1u8; 20]))
            .to(EthAddress::from([2u8; 20]))
            .data(Bytes::from_static(&[0xde, 0xad, 0xbe, 0xef]))
            .build();

        let receipt = EthReceipt::new(
            true,
            Some(EthAddress::from([3u8; 20])),
            vec![EthLog::new(
                EthAddress::from([4u8; 20]),
                vec![B256::with_last_byte(9), B256::with_last_byte(10)],
                Bytes::from_static(&[0x01, 0x02]),
            )],
        );

        MinedTx::new(tx, receipt)
    }

    #[test]
    fn a_row_survives_the_round_trip() {
        let before = sample();
        let after = TxRow::from_mined(&before).into_mined().unwrap();

        assert_eq!(after.tx().tx_hash(), before.tx().tx_hash());
        assert_eq!(after.tx().block_number(), before.tx().block_number());
        assert_eq!(after.tx().timestamp(), before.tx().timestamp());
        assert_eq!(after.tx().amount(), before.tx().amount());
        assert_eq!(after.tx().from(), before.tx().from());
        assert_eq!(after.tx().to(), before.tx().to());
        assert_eq!(after.tx().data(), before.tx().data());
        assert_eq!(after.receipt().succeeded(), before.receipt().succeeded());
        assert_eq!(
            after.receipt().contract_address(),
            before.receipt().contract_address()
        );
        assert_eq!(after.receipt().logs().len(), 1);
        assert_eq!(
            after.receipt().logs()[0].topics(),
            before.receipt().logs()[0].topics()
        );
        assert_eq!(
            after.receipt().logs()[0].data(),
            before.receipt().logs()[0].data()
        );
    }

    #[test]
    fn a_contractless_tx_round_trips_too() {
        let tx = EthTx::builder()
            .tx_hash(TxHash::ZERO)
            .block_number(1)
            .timestamp(2)
            .amount(U256::ZERO)
            .from(EthAddress::ZERO)
            .data(Bytes::new())
            .build();

        let mined = MinedTx::new(tx, EthReceipt::new(false, None, Vec::new()));
        let after = TxRow::from_mined(&mined).into_mined().unwrap();

        assert!(after.tx().to().is_none());
        assert!(!after.receipt().succeeded());
        assert!(after.receipt().contract_address().is_none());
    }

    #[test]
    fn json_each_row_keeps_the_column_names() {
        let encoded = serde_json::to_string(&TxRow::from_mined(&sample())).unwrap();

        assert!(encoded.contains("\"block_number\":21000000"));
        assert!(encoded.contains("\"succeeded\":1"));
        assert!(encoded.contains("\"log_topics\":[["));
    }
}
