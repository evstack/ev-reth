//! TOML config types, parsing, and validation.

use serde::Deserialize;
use std::path::Path;

/// Top-level deploy configuration.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct DeployConfig {
    /// Chain configuration.
    pub chain: ChainConfig,
    /// Contract configurations.
    pub contracts: ContractsConfig,
}

/// Chain-level settings.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct ChainConfig {
    /// The chain ID.
    pub chain_id: u64,
}

/// All contract configurations.
#[derive(Debug, Deserialize, Default)]
pub(crate) struct ContractsConfig {}

impl DeployConfig {
    /// Load and validate config from a TOML file.
    pub(crate) fn load(path: &Path) -> eyre::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate config values.
    fn validate(&self) -> eyre::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_config() {
        let toml = r#"
[chain]
chain_id = 1234

[contracts]
"#;
        let config: DeployConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.chain.chain_id, 1234);
        config.validate().unwrap();
    }
}
