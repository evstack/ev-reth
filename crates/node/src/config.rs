use serde::{Deserialize, Serialize};

/// Configuration for the Rollkit payload builder
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RollkitPayloadBuilderConfig {}

impl RollkitPayloadBuilderConfig {
    /// Creates a new instance of `RollkitPayloadBuilderConfig`
    pub const fn new() -> Self {
        Self {}
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
}
