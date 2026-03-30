//! Deploy state file: tracks deployment progress with resumability.

use crate::config::DeployConfig;
use alloy_primitives::{Address, B256};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Current state file schema version.
const STATE_VERSION: u32 = 1;

/// Overall deployment state, persisted to JSON.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DeployState {
    /// Schema version.
    pub version: u32,
    /// Random salt for CREATE2 deployments.
    pub create2_salt: B256,
    /// Snapshot of the config at first run — used for immutability checks.
    pub applied_intent: AppliedIntent,
    /// Per-contract deployment state.
    #[serde(default)]
    pub contracts: ContractStates,
}

/// Snapshot of the config that was used for the first deployment.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct AppliedIntent {
    pub chain_id: u64,
    pub admin_proxy: Option<AppliedAdminProxy>,
    pub permit2: Option<AppliedPermit2>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct AppliedAdminProxy {
    pub owner: Address,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct AppliedPermit2 {}

/// Per-contract deployment states.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct ContractStates {
    pub admin_proxy: Option<ContractState>,
    pub permit2: Option<ContractState>,
}

/// State of a single contract deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ContractState {
    pub status: ContractStatus,
    pub address: Address,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deploy_tx: Option<B256>,
}

/// Contract deployment status progression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ContractStatus {
    Pending,
    Deployed,
    Verified,
}

impl DeployState {
    /// Create a new state from config, generating a random salt.
    pub(crate) fn new(config: &DeployConfig) -> Self {
        let mut salt_bytes = [0u8; 32];
        let mut rng = rand::rng();
        rng.fill(&mut salt_bytes);
        let salt = B256::from(salt_bytes);

        Self {
            version: STATE_VERSION,
            create2_salt: salt,
            applied_intent: AppliedIntent::from_config(config),
            contracts: ContractStates::default(),
        }
    }

    /// Load state from a JSON file.
    pub(crate) fn load(path: &Path) -> eyre::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let state: Self = serde_json::from_str(&content)?;
        eyre::ensure!(
            state.version == STATE_VERSION,
            "unsupported state version: {} (expected {})",
            state.version,
            STATE_VERSION
        );
        Ok(state)
    }

    /// Save state to a JSON file.
    pub(crate) fn save(&self, path: &Path) -> eyre::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Validate that the current config is compatible with the applied intent.
    /// Immutable fields cannot change. New contracts can be added.
    pub(crate) fn validate_immutability(&self, config: &DeployConfig) -> eyre::Result<()> {
        let current = &self.applied_intent;

        eyre::ensure!(
            config.chain.chain_id == current.chain_id,
            "immutability violation: chain_id changed from {} to {}",
            current.chain_id,
            config.chain.chain_id
        );

        // If admin_proxy was in the original intent, its owner must not change
        if let Some(ref original_ap) = current.admin_proxy {
            if let Some(ref new_ap) = config.contracts.admin_proxy {
                eyre::ensure!(
                    new_ap.owner == original_ap.owner,
                    "immutability violation: admin_proxy.owner changed from {} to {}",
                    original_ap.owner,
                    new_ap.owner
                );
            } else {
                eyre::bail!(
                    "immutability violation: admin_proxy was configured but is now missing"
                );
            }
        }

        // If permit2 was in the original intent, it must still be present
        if current.permit2.is_some() {
            eyre::ensure!(
                config.contracts.permit2.is_some(),
                "immutability violation: permit2 was configured but is now missing"
            );
        }

        Ok(())
    }
}

impl AppliedIntent {
    fn from_config(config: &DeployConfig) -> Self {
        Self {
            chain_id: config.chain.chain_id,
            admin_proxy: config
                .contracts
                .admin_proxy
                .as_ref()
                .map(|ap| AppliedAdminProxy { owner: ap.owner }),
            permit2: config.contracts.permit2.as_ref().map(|_| AppliedPermit2 {}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;
    use alloy_primitives::address;

    fn test_config() -> DeployConfig {
        DeployConfig {
            chain: ChainConfig { chain_id: 1234 },
            contracts: ContractsConfig {
                admin_proxy: Some(AdminProxyConfig {
                    address: None,
                    owner: address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
                }),
                permit2: Some(Permit2Config { address: None }),
            },
        }
    }

    #[test]
    fn new_state_has_random_salt() {
        let s1 = DeployState::new(&test_config());
        let s2 = DeployState::new(&test_config());
        assert_ne!(s1.create2_salt, s2.create2_salt);
    }

    #[test]
    fn new_state_snapshots_intent() {
        let state = DeployState::new(&test_config());
        assert_eq!(state.applied_intent.chain_id, 1234);
        assert!(state.applied_intent.admin_proxy.is_some());
        assert!(state.applied_intent.permit2.is_some());
    }

    #[test]
    fn roundtrip_save_load() {
        let state = DeployState::new(&test_config());
        let tmp = tempfile::NamedTempFile::new().unwrap();
        state.save(tmp.path()).unwrap();
        let loaded = DeployState::load(tmp.path()).unwrap();
        assert_eq!(loaded.create2_salt, state.create2_salt);
        assert_eq!(loaded.applied_intent, state.applied_intent);
    }

    #[test]
    fn immutability_ok_same_config() {
        let config = test_config();
        let state = DeployState::new(&config);
        assert!(state.validate_immutability(&config).is_ok());
    }

    #[test]
    fn immutability_rejects_chain_id_change() {
        let config = test_config();
        let state = DeployState::new(&config);
        let mut changed = config.clone();
        changed.chain.chain_id = 9999;
        let err = state
            .validate_immutability(&changed)
            .unwrap_err()
            .to_string();
        assert!(err.contains("chain_id changed"), "{err}");
    }

    #[test]
    fn immutability_rejects_owner_change() {
        let config = test_config();
        let state = DeployState::new(&config);
        let mut changed = config.clone();
        changed.contracts.admin_proxy.as_mut().unwrap().owner =
            address!("0000000000000000000000000000000000000001");
        let err = state
            .validate_immutability(&changed)
            .unwrap_err()
            .to_string();
        assert!(err.contains("admin_proxy.owner changed"), "{err}");
    }

    #[test]
    fn immutability_allows_adding_new_contract() {
        let config = DeployConfig {
            chain: ChainConfig { chain_id: 1234 },
            contracts: ContractsConfig {
                admin_proxy: Some(AdminProxyConfig {
                    address: None,
                    owner: address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
                }),
                permit2: None,
            },
        };
        let state = DeployState::new(&config);

        // Now add permit2 — this should be allowed
        let mut extended = config.clone();
        extended.contracts.permit2 = Some(Permit2Config { address: None });
        assert!(state.validate_immutability(&extended).is_ok());
    }

    #[test]
    fn immutability_rejects_removing_contract() {
        let config = test_config();
        let state = DeployState::new(&config);
        let mut changed = config.clone();
        changed.contracts.admin_proxy = None;
        let err = state
            .validate_immutability(&changed)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("admin_proxy was configured but is now missing"),
            "{err}"
        );
    }

    #[test]
    fn contract_status_ordering() {
        assert!(ContractStatus::Pending < ContractStatus::Deployed);
        assert!(ContractStatus::Deployed < ContractStatus::Verified);
    }
}
