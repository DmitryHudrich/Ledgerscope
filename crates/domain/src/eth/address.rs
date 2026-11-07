use std::{fmt, str::FromStr};

use alloy_primitives::{Address, B256};

#[derive(Eq, Copy, Clone, Hash, PartialEq)]
pub struct EthAddress(Address);

impl std::fmt::Debug for EthAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("EthAddress").field(&self.hex()).finish()
    }
}

#[derive(Debug)]
pub struct EthAddressParseError(String);

impl fmt::Display for EthAddressParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid eth address: {}", self.0)
    }
}

impl std::error::Error for EthAddressParseError {}

impl FromStr for EthAddress {
    type Err = EthAddressParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Address::from_str(s)
            .map(EthAddress)
            .map_err(|e| EthAddressParseError(e.to_string()))
    }
}

impl From<Address> for EthAddress {
    fn from(address: Address) -> Self {
        EthAddress(address)
    }
}

impl From<[u8; 20]> for EthAddress {
    fn from(bytes: [u8; 20]) -> Self {
        EthAddress(Address::from(bytes))
    }
}

impl TryFrom<Vec<u8>> for EthAddress {
    type Error = EthAddressParseError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        Address::try_from(bytes.as_slice())
            .map(EthAddress)
            .map_err(|_| EthAddressParseError(format!("expected 20 bytes, got {}", bytes.len())))
    }
}

impl fmt::Display for EthAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", self.hex())
    }
}

impl EthAddress {
    pub const ZERO: Self = EthAddress(Address::ZERO);

    pub fn from_word(word: B256) -> Self {
        EthAddress(Address::from_word(word))
    }

    pub fn address(&self) -> Address {
        self.0
    }

    pub fn hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_tron_hex(raw: &str) -> Result<Self, EthAddressParseError> {
        let raw = raw.trim().trim_start_matches("0x");
        let raw = match raw.len() {
            42 if raw[..2].eq_ignore_ascii_case(TRON_PREFIX_HEX) => &raw[2..],
            40 => raw,
            _ => {
                return Err(EthAddressParseError(format!(
                    "{raw} is not a tron hex address"
                )));
            }
        };

        raw.parse::<Address>()
            .map(EthAddress)
            .map_err(|error| EthAddressParseError(error.to_string()))
    }

    pub fn from_tron_base58(raw: &str) -> Result<Self, EthAddressParseError> {
        let bytes = bs58::decode(raw.trim())
            .with_check(Some(TRON_PREFIX))
            .into_vec()
            .map_err(|error| EthAddressParseError(format!("{raw}: {error}")))?;

        Self::try_from(bytes[1..].to_vec())
    }

    pub fn tron_hex(&self) -> String {
        format!("{TRON_PREFIX_HEX}{}", self.hex())
    }

    pub fn tron_base58(&self) -> String {
        let mut bytes = Vec::with_capacity(21);
        bytes.push(TRON_PREFIX);
        bytes.extend_from_slice(self.0.as_slice());
        bs58::encode(bytes).with_check().into_string()
    }

    pub fn parse_any(raw: &str) -> Result<Self, EthAddressParseError> {
        let raw = raw.trim();
        if raw.starts_with('T') {
            return Self::from_tron_base58(raw);
        }
        let hex = raw.trim_start_matches("0x");
        if hex.len() == 42 {
            return Self::from_tron_hex(hex);
        }
        raw.parse()
    }
}

const TRON_PREFIX: u8 = 0x41;
const TRON_PREFIX_HEX: &str = "41";

#[cfg(test)]
mod tron_tests {
    use super::*;

    const USDT_BASE58: &str = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t";
    const USDT_HEX: &str = "41a614f803b6fd780986a42c78ec9c7f77e6ded13c";

    #[test]
    fn a_tron_address_survives_both_spellings() {
        let from_base58 = EthAddress::from_tron_base58(USDT_BASE58).unwrap();
        let from_hex = EthAddress::from_tron_hex(USDT_HEX).unwrap();

        assert_eq!(from_base58, from_hex);
        assert_eq!(from_hex.tron_base58(), USDT_BASE58);
        assert_eq!(from_hex.tron_hex(), USDT_HEX);
        assert_eq!(
            EthAddress::parse_any(USDT_BASE58).unwrap().to_string(),
            "0xa614f803b6fd780986a42c78ec9c7f77e6ded13c"
        );
    }

    #[test]
    fn a_broken_checksum_is_turned_away() {
        assert!(EthAddress::from_tron_base58("TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6u").is_err());
    }
}
