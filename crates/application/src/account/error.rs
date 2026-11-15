use std::{fmt, io};

use domain::account::ValueError;

#[derive(Debug)]
pub enum StoreError {
    Conflict(String),
    Io(io::Error),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflict(message) => f.write_str(message),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
pub enum AccountError {
    Invalid(String),
    Unauthenticated,
    Forbidden(String),
    NotFound(&'static str),
    Conflict(String),
    Storage(io::Error),
}

impl AccountError {
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

impl fmt::Display for AccountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) | Self::Forbidden(message) | Self::Conflict(message) => {
                f.write_str(message)
            }
            Self::Unauthenticated => f.write_str("sign in first"),
            Self::NotFound(what) => write!(f, "{what} not found"),
            Self::Storage(error) => write!(f, "storage failed: {error}"),
        }
    }
}

impl std::error::Error for AccountError {}

impl From<ValueError> for AccountError {
    fn from(error: ValueError) -> Self {
        Self::Invalid(error.to_string())
    }
}

impl From<StoreError> for AccountError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Conflict(message) => Self::Conflict(message),
            StoreError::Io(error) => Self::Storage(error),
        }
    }
}

impl From<io::Error> for AccountError {
    fn from(error: io::Error) -> Self {
        Self::Storage(error)
    }
}
