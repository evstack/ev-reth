//! Configuration helpers for enabling base-fee redirection.

use alloy_primitives::Address;
use std::{env, fmt, str::FromStr};
use thiserror::Error;

/// User-facing configuration for the base-fee sink address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseFeeConfig {
    /// Address receiving the base-fee proceeds.
    pub fee_reciever: Address,
}

impl BaseFeeConfig {
    /// Constructs a new configuration from the provided sink address.
    pub const fn new(fee_reciever: Address) -> Self {
        Self { fee_reciever }
    }

    /// Returns the configured sink address.
    pub const fn fee_reciever(&self) -> Address {
        self.fee_reciever
    }

    /// Parses a configuration from a string representation of an address.
    pub fn from_str(value: &str) -> Result<Self, ConfigError> {
        parse_address(value).map(Self::new)
    }

    /// Loads the configuration from an environment variable.
    ///
    /// The variable must contain a hex-encoded address (with or without a `0x` prefix).
    pub fn from_env(var: &str) -> Result<Self, ConfigError> {
        let raw = env::var(var).map_err(|_| ConfigError::MissingEnv { var: var.into() })?;
        if raw.trim().is_empty() {
            return Err(ConfigError::EmptyEnv { var: var.into() });
        }
        Self::from_str(raw.trim())
    }
}

impl From<Address> for BaseFeeConfig {
    fn from(address: Address) -> Self {
        Self::new(address)
    }
}

/// Errors that can occur while building a [`BaseFeeConfig`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    /// The sink address environment variable was not set.
    #[error("environment variable {var} is not set")]
    MissingEnv {
        /// Name of the environment variable that was not present.
        var: String,
    },
    /// The sink address environment variable was empty or whitespace.
    #[error("environment variable {var} is empty")]
    EmptyEnv {
        /// Name of the environment variable that evaluated to an empty string.
        var: String,
    },
    /// The supplied address could not be parsed.
    #[error("invalid fee sink address: {0}")]
    InvalidAddress(AddressParseDisplay),
}

/// Wrapper for formatting address parse failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressParseDisplay(String);

impl fmt::Display for AddressParseDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn parse_address(value: &str) -> Result<Address, ConfigError> {
    Address::from_str(value)
        .map_err(|err| ConfigError::InvalidAddress(AddressParseDisplay(err.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn parses_address_from_str() {
        let cfg = BaseFeeConfig::from_str("0x00000000000000000000000000000000000000aa").unwrap();
        assert_eq!(
            cfg.fee_reciever,
            address!("0x00000000000000000000000000000000000000aa")
        );
    }

    #[test]
    fn rejects_invalid_address() {
        let err = BaseFeeConfig::from_str("not_an_address").unwrap_err();
        assert!(matches!(err, ConfigError::InvalidAddress(_)));
    }
}
