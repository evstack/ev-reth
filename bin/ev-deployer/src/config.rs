//! TOML config types, parsing, and validation.

use alloy_primitives::{Address, B256};
use serde::Deserialize;
use std::path::Path;

/// Top-level deploy configuration.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct DeployConfig {
    /// Chain configuration.
    pub chain: ChainConfig,
    /// Contract configurations.
    pub contracts: ContractsConfig,
}

/// Chain-level settings.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ChainConfig {
    /// The chain ID.
    pub chain_id: u64,
}

/// All contract configurations.
#[derive(Debug, Deserialize)]
pub struct ContractsConfig {
    /// `AdminProxy` contract config (optional).
    pub admin_proxy: Option<AdminProxyConfig>,
    /// `FeeVault` contract config (optional).
    pub fee_vault: Option<FeeVaultConfig>,
    /// `MerkleTreeHook` contract config (optional).
    pub merkle_tree_hook: Option<MerkleTreeHookConfig>,
    /// `Mailbox` contract config (optional).
    pub mailbox: Option<MailboxConfig>,
    /// `NoopIsm` contract config (optional).
    pub noop_ism: Option<NoopIsmConfig>,
    /// `Permit2` contract config (optional).
    pub permit2: Option<Permit2Config>,
    /// `ProtocolFee` contract config (optional).
    pub protocol_fee: Option<ProtocolFeeConfig>,
}

/// `AdminProxy` configuration.
#[derive(Debug, Deserialize)]
pub struct AdminProxyConfig {
    /// Address to deploy at.
    pub address: Address,
    /// Owner address.
    pub owner: Address,
}

/// `FeeVault` configuration.
#[derive(Debug, Deserialize)]
pub struct FeeVaultConfig {
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

/// `MerkleTreeHook` configuration (Hyperlane required hook).
#[derive(Debug, Deserialize)]
pub struct MerkleTreeHookConfig {
    /// Address to deploy at.
    pub address: Address,
    /// Owner address (for post-genesis hook/ISM changes).
    #[serde(default)]
    pub owner: Address,
    /// Mailbox address (patched into bytecode as immutable).
    pub mailbox: Address,
}

/// `MailboxConfig` configuration (Hyperlane core messaging hub).
#[derive(Debug, Deserialize)]
pub struct MailboxConfig {
    /// Address to deploy at.
    pub address: Address,
    /// Owner address.
    #[serde(default)]
    pub owner: Address,
    /// Default interchain security module.
    #[serde(default)]
    pub default_ism: Address,
    /// Default post-dispatch hook.
    #[serde(default)]
    pub default_hook: Address,
    /// Required post-dispatch hook (e.g. `MerkleTreeHook`).
    #[serde(default)]
    pub required_hook: Address,
}

/// `NoopIsm` configuration (Hyperlane ISM that accepts all messages).
#[derive(Debug, Deserialize)]
pub struct NoopIsmConfig {
    /// Address to deploy at.
    pub address: Address,
}

/// `Permit2` configuration (Uniswap token approval manager).
#[derive(Debug, Deserialize)]
pub struct Permit2Config {
    /// Address to deploy at.
    pub address: Address,
}

/// `ProtocolFee` configuration (Hyperlane post-dispatch hook that charges a protocol fee).
#[derive(Debug, Deserialize)]
pub struct ProtocolFeeConfig {
    /// Address to deploy at.
    pub address: Address,
    /// Owner address.
    #[serde(default)]
    pub owner: Address,
    /// Maximum protocol fee in wei.
    pub max_protocol_fee: u64,
    /// Protocol fee charged per dispatch in wei.
    #[serde(default)]
    pub protocol_fee: u64,
    /// Beneficiary address that receives collected fees.
    #[serde(default)]
    pub beneficiary: Address,
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

        if let Some(ref mth) = self.contracts.merkle_tree_hook {
            eyre::ensure!(
                !mth.mailbox.is_zero(),
                "merkle_tree_hook.mailbox must not be the zero address"
            );
        }

        if let Some(ref pf) = self.contracts.protocol_fee {
            eyre::ensure!(
                !pf.owner.is_zero(),
                "protocol_fee.owner must not be the zero address"
            );
            eyre::ensure!(
                !pf.beneficiary.is_zero(),
                "protocol_fee.beneficiary must not be the zero address"
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
    fn parse_merkle_tree_hook_config() {
        let toml = r#"
[chain]
chain_id = 1234

[contracts.merkle_tree_hook]
address = "0x0000000000000000000000000000000000001100"
owner = "0x000000000000000000000000000000000000ad00"
mailbox = "0x0000000000000000000000000000000000001200"
"#;
        let config: DeployConfig = toml::from_str(toml).unwrap();
        config.validate().unwrap();
        assert!(config.contracts.merkle_tree_hook.is_some());
        let mth = config.contracts.merkle_tree_hook.unwrap();
        assert!(!mth.mailbox.is_zero());
    }

    #[test]
    fn reject_zero_mailbox_merkle_tree_hook() {
        let toml = r#"
[chain]
chain_id = 1

[contracts.merkle_tree_hook]
address = "0x0000000000000000000000000000000000001100"
mailbox = "0x0000000000000000000000000000000000000000"
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
