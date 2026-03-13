//! TOML config types, parsing, and validation.

use alloy_primitives::{Address, B256};
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
#[derive(Debug, Deserialize)]
pub(crate) struct ContractsConfig {
    /// `AdminProxy` contract config (optional).
    pub admin_proxy: Option<AdminProxyConfig>,
    /// `FeeVault` contract config (optional).
    pub fee_vault: Option<FeeVaultConfig>,
}

/// `AdminProxy` configuration.
#[derive(Debug, Deserialize)]
pub(crate) struct AdminProxyConfig {
    /// Address to deploy at.
    pub address: Address,
    /// Owner address.
    pub owner: Address,
}

/// `FeeVault` configuration.
#[derive(Debug, Deserialize)]
pub(crate) struct FeeVaultConfig {
    /// Address to deploy at.
    pub address: Address,
    /// Owner address.
    pub owner: Address,
    /// Hyperlane destination domain.
    #[serde(default)]
    pub destination_domain: u32,
    /// Hyperlane recipient address (bytes32).
    #[serde(default)]
    pub recipient_address: B256,
    /// Minimum amount for bridging.
    #[serde(default)]
    pub minimum_amount: u64,
    /// Call fee for sendToCelestia.
    #[serde(default)]
    pub call_fee: u64,
    /// Basis points for bridge share (0-10000). 0 defaults to 10000.
    #[serde(default)]
    pub bridge_share_bps: u64,
    /// Other recipient for split accounting.
    #[serde(default)]
    pub other_recipient: Address,
    /// `HypNativeMinter` address.
    #[serde(default)]
    pub hyp_native_minter: Address,
}

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
        if let Some(ref ap) = self.contracts.admin_proxy {
            eyre::ensure!(
                !ap.owner.is_zero(),
                "admin_proxy.owner must not be the zero address"
            );
        }

        if let Some(ref fv) = self.contracts.fee_vault {
            eyre::ensure!(
                !fv.owner.is_zero(),
                "fee_vault.owner must not be the zero address"
            );
            eyre::ensure!(
                fv.bridge_share_bps <= 10000,
                "fee_vault.bridge_share_bps must be 0-10000, got {}",
                fv.bridge_share_bps
            );
        }

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

[contracts.admin_proxy]
address = "0x000000000000000000000000000000000000Ad00"
owner = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"

[contracts.fee_vault]
address = "0x000000000000000000000000000000000000FE00"
owner = "0x000000000000000000000000000000000000Ad00"
destination_domain = 0
recipient_address = "0x0000000000000000000000000000000000000000000000000000000000000000"
minimum_amount = 0
call_fee = 0
bridge_share_bps = 10000
other_recipient = "0x0000000000000000000000000000000000000000"
hyp_native_minter = "0x0000000000000000000000000000000000000000"
"#;
        let config: DeployConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.chain.chain_id, 1234);
        assert!(config.contracts.admin_proxy.is_some());
        assert!(config.contracts.fee_vault.is_some());
        config.validate().unwrap();
    }

    #[test]
    fn reject_zero_owner() {
        let toml = r#"
[chain]
chain_id = 1

[contracts.admin_proxy]
address = "0x000000000000000000000000000000000000Ad00"
owner = "0x0000000000000000000000000000000000000000"
"#;
        let config: DeployConfig = toml::from_str(toml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn reject_bps_over_10000() {
        let toml = r#"
[chain]
chain_id = 1

[contracts.fee_vault]
address = "0x000000000000000000000000000000000000FE00"
owner = "0x000000000000000000000000000000000000Ad00"
bridge_share_bps = 10001
"#;
        let config: DeployConfig = toml::from_str(toml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn admin_proxy_only() {
        let toml = r#"
[chain]
chain_id = 1

[contracts.admin_proxy]
address = "0x000000000000000000000000000000000000Ad00"
owner = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
"#;
        let config: DeployConfig = toml::from_str(toml).unwrap();
        config.validate().unwrap();
        assert!(config.contracts.admin_proxy.is_some());
        assert!(config.contracts.fee_vault.is_none());
    }
}
