use alloy_primitives::Address;
use reth_chainspec::ChainSpec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ChainspecEvolveConfig {
    #[serde(default, rename = "baseFeeSink")]
    pub base_fee_sink: Option<Address>,
}

/// Configuration for the Evolve payload builder
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvolvePayloadBuilderConfig {
    /// Optional chainspec-configured recipient for redirected base fees.
    #[serde(default)]
    pub base_fee_sink: Option<Address>,
}

impl EvolvePayloadBuilderConfig {
    /// Creates a new instance of `EvolvePayloadBuilderConfig`
    pub const fn new() -> Self {
        Self {
            base_fee_sink: None,
        }
    }

    /// Builds the configuration from the provided chain spec extras.
    pub fn from_chain_spec(spec: &ChainSpec) -> Result<Self, ConfigError> {
        let mut config = Self::default();
        if let Some(extra) = spec
            .genesis
            .config
            .extra_fields
            .get_deserialized::<ChainspecEvolveConfig>("ev_reth")
        {
            let extras = extra.map_err(ConfigError::InvalidExtras)?;
            config.base_fee_sink = extras.base_fee_sink;
        }
        Ok(config)
    }

    /// Validates the configuration
    pub const fn validate(&self) -> Result<(), ConfigError> {
        Ok(())
    }
}

/// Errors that can occur during configuration validation
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Invalid configuration provided
    #[error("Invalid config")]
    InvalidConfig,
    /// Chainspec extras contained invalid values
    #[error("Invalid ev_reth extras in chainspec: {0}")]
    InvalidExtras(#[from] serde_json::Error),
}
