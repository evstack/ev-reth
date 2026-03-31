//! TOML config types, parsing, and validation.

use alloy_primitives::Address;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};

/// Top-level deploy configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeployConfig {
    /// Chain configuration.
    pub chain: ChainConfig,
    /// Contract configurations.
    #[serde(default)]
    pub contracts: ContractsConfig,
}

/// Chain-level settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChainConfig {
    /// The chain ID.
    pub chain_id: u64,
}

/// All contract configurations.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ContractsConfig {
    /// `AdminProxy` contract config (optional).
    pub admin_proxy: Option<AdminProxyConfig>,
    /// `Permit2` contract config (optional).
    pub permit2: Option<Permit2Config>,
}

impl ContractsConfig {
    /// Collect all configured deploy addresses.
    fn all_addresses(&self) -> Vec<Address> {
        let mut addrs = Vec::new();
        if let Some(ref ap) = self.admin_proxy {
            if let Some(addr) = ap.address {
                addrs.push(addr);
            }
        }
        if let Some(ref p2) = self.permit2 {
            if let Some(addr) = p2.address {
                addrs.push(addr);
            }
        }
        addrs
    }
}

/// `AdminProxy` configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdminProxyConfig {
    /// Address to deploy at (required for genesis, ignored for deploy).
    pub address: Option<Address>,
    /// Owner address.
    pub owner: Address,
}

/// `Permit2` configuration (Uniswap token approval manager).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Permit2Config {
    /// Address to deploy at (required for genesis, ignored for deploy).
    pub address: Option<Address>,
}

impl DeployConfig {
    /// Load and validate config from a TOML file.
    pub fn load(path: &Path) -> eyre::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Validate config values.
    pub(crate) fn validate(&self) -> eyre::Result<()> {
        if let Some(ref ap) = self.contracts.admin_proxy {
            eyre::ensure!(
                !ap.owner.is_zero(),
                "admin_proxy.owner must not be the zero address"
            );
        }

        if let Some(ref p2) = self.contracts.permit2 {
            if let Some(addr) = p2.address {
                eyre::ensure!(
                    !addr.is_zero(),
                    "permit2.address must not be the zero address"
                );
            }
        }

        // Detect duplicate deploy addresses across all contracts.
        let mut seen = HashSet::new();
        for addr in self.contracts.all_addresses() {
            eyre::ensure!(seen.insert(addr), "duplicate deploy address: {addr}");
        }

        Ok(())
    }

    /// Additional validation for genesis mode: all addresses must be specified.
    pub fn validate_for_genesis(&self) -> eyre::Result<()> {
        if let Some(ref ap) = self.contracts.admin_proxy {
            eyre::ensure!(
                ap.address.is_some(),
                "admin_proxy.address is required for genesis mode"
            );
        }
        if let Some(ref p2) = self.contracts.permit2 {
            eyre::ensure!(
                p2.address.is_some(),
                "permit2.address is required for genesis mode"
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
"#;
        let config: DeployConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.chain.chain_id, 1234);
        assert!(config.contracts.admin_proxy.is_some());
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
    fn no_contracts_section() {
        let toml = r#"
[chain]
chain_id = 1
"#;
        let config: DeployConfig = toml::from_str(toml).unwrap();
        config.validate().unwrap();
        assert!(config.contracts.admin_proxy.is_none());
    }

    #[test]
    fn reject_zero_permit2_address() {
        let toml = r#"
[chain]
chain_id = 1

[contracts.permit2]
address = "0x0000000000000000000000000000000000000000"
"#;
        let config: DeployConfig = toml::from_str(toml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn reject_duplicate_deploy_address() {
        let toml = r#"
[chain]
chain_id = 1

[contracts.admin_proxy]
address = "0x000000000000000000000000000000000000Ad00"
owner = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"

[contracts.permit2]
address = "0x000000000000000000000000000000000000Ad00"
"#;
        let config: DeployConfig = toml::from_str(toml).unwrap();
        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("duplicate deploy address"), "{err}");
    }

    #[test]
    fn permit2_only() {
        let toml = r#"
[chain]
chain_id = 1

[contracts.permit2]
address = "0x000000000022D473030F116dDEE9F6B43aC78BA3"
"#;
        let config: DeployConfig = toml::from_str(toml).unwrap();
        config.validate().unwrap();
        assert!(config.contracts.permit2.is_some());
        assert!(config.contracts.admin_proxy.is_none());
    }

    #[test]
    fn both_contracts() {
        let toml = r#"
[chain]
chain_id = 1

[contracts.admin_proxy]
address = "0x000000000000000000000000000000000000Ad00"
owner = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"

[contracts.permit2]
address = "0x000000000022D473030F116dDEE9F6B43aC78BA3"
"#;
        let config: DeployConfig = toml::from_str(toml).unwrap();
        config.validate().unwrap();
        assert!(config.contracts.admin_proxy.is_some());
        assert!(config.contracts.permit2.is_some());
    }

    #[test]
    fn reject_missing_address_for_genesis() {
        use alloy_primitives::address;

        let config = DeployConfig {
            chain: ChainConfig { chain_id: 1 },
            contracts: ContractsConfig {
                admin_proxy: Some(AdminProxyConfig {
                    address: None,
                    owner: address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
                }),
                permit2: None,
            },
        };
        config.validate().unwrap(); // base validation passes
        assert!(config.validate_for_genesis().is_err());
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
    }
}
