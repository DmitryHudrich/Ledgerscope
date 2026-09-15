use std::{fmt, str::FromStr};

#[derive(Eq, Clone, Hash, PartialEq)]
pub struct EthAddress(Vec<u8>);

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
        let stripped = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(stripped).map_err(|e| EthAddressParseError(e.to_string()))?;

        if bytes.len() != 20 {
            return Err(EthAddressParseError(format!(
                "expected 20 bytes, got {}",
                bytes.len()
            )));
        }

        Ok(EthAddress(bytes))
    }
}

impl From<[u8; 20]> for EthAddress {
    fn from(bytes: [u8; 20]) -> Self {
        EthAddress(bytes.to_vec())
    }
}

impl TryFrom<Vec<u8>> for EthAddress {
    type Error = EthAddressParseError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        if bytes.len() != 20 {
            return Err(EthAddressParseError(format!(
                "expected 20 bytes, got {}",
                bytes.len()
            )));
        }
        Ok(EthAddress(bytes))
    }
}

impl fmt::Display for EthAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(&self.0))
    }
}

impl EthAddress {
    pub fn hex(&self) -> String {
        hex::encode(&self.0)
    }
}
