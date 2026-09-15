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

    pub fn checksummed(&self) -> String {
        self.0.to_checksum(None)
    }
}
