use alloy_primitives::{Address, B256};
use reth_chainspec::ChainSpec;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Default contract size limit in bytes (24KB per EIP-170).
pub const DEFAULT_CONTRACT_SIZE_LIMIT: usize = 24 * 1024;
/// Maximum number of addresses allowed in the deploy allowlist.
pub const MAX_DEPLOY_ALLOWLIST_LEN: usize = ev_revm::MAX_DEPLOYERS;

/// Deployment allowlist configuration derived from the chainspec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployAllowlistConfig {
    /// Genesis deployer baseline.
    pub baseline: Vec<Address>,
    /// Block height at which deployment restrictions activate.
    pub activation_height: u64,
    /// Optional fixed admin for state-backed policy management.
    pub admin: Option<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ChainspecEvolveConfig {
    #[serde(default, rename = "baseFeeSink")]
    pub base_fee_sink: Option<Address>,
    #[serde(default, rename = "baseFeeRedirectActivationHeight")]
    pub base_fee_redirect_activation_height: Option<u64>,
    #[serde(default, rename = "mintAdmin")]
    pub mint_admin: Option<Address>,
    #[serde(default, rename = "mintPrecompileActivationHeight")]
    pub mint_precompile_activation_height: Option<u64>,
    #[serde(default, rename = "proposerControlAdmin")]
    pub proposer_control_admin: Option<Address>,
    #[serde(default, rename = "proposerControlActivationHeight")]
    pub proposer_control_activation_height: Option<u64>,
    #[serde(default, rename = "initialNextProposer")]
    pub initial_next_proposer: Option<B256>,
    /// Maximum contract code size in bytes. Defaults to 24KB (EIP-170) if not specified.
    #[serde(default, rename = "contractSizeLimit")]
    pub contract_size_limit: Option<usize>,
    /// Block height at which the custom contract size limit activates.
    #[serde(default, rename = "contractSizeLimitActivationHeight")]
    pub contract_size_limit_activation_height: Option<u64>,
    /// Optional allowlist of addresses permitted to deploy contracts.
    #[serde(default, rename = "deployAllowlist")]
    pub deploy_allowlist: Option<Vec<Address>>,
    /// Block height at which deploy allowlist enforcement activates.
    #[serde(default, rename = "deployAllowlistActivationHeight")]
    pub deploy_allowlist_activation_height: Option<u64>,
    /// Fixed admin for the state-backed deployment-permissions precompile.
    #[serde(default, rename = "deployAllowlistAdmin")]
    pub deploy_allowlist_admin: Option<Address>,
}

/// Configuration for the Evolve payload builder
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvolvePayloadBuilderConfig {
    /// Optional chainspec-configured recipient for redirected base fees.
    #[serde(default)]
    pub base_fee_sink: Option<Address>,
    /// Optional activation height for base-fee redirect; defaults to 0 when sink set.
    #[serde(default)]
    pub base_fee_redirect_activation_height: Option<u64>,
    /// Optional mint precompile admin address sourced from the chainspec.
    #[serde(default)]
    pub mint_admin: Option<Address>,
    /// Optional activation height for mint precompile; defaults to 0 when admin set.
    #[serde(default)]
    pub mint_precompile_activation_height: Option<u64>,
    /// Optional proposer control precompile admin address sourced from the chainspec.
    #[serde(default)]
    pub proposer_control_admin: Option<Address>,
    /// Optional activation height for proposer control precompile; defaults to 0 when admin set.
    #[serde(default)]
    pub proposer_control_activation_height: Option<u64>,
    /// Optional initial next proposer returned until the precompile storage is updated.
    #[serde(default)]
    pub initial_next_proposer: Option<B256>,
    /// Maximum contract code size in bytes. Defaults to 24KB (EIP-170).
    #[serde(default)]
    pub contract_size_limit: Option<usize>,
    /// Block height at which the custom contract size limit activates.
    #[serde(default)]
    pub contract_size_limit_activation_height: Option<u64>,
    /// Allowlist of addresses permitted to deploy contracts.
    #[serde(default)]
    pub deploy_allowlist: Vec<Address>,
    /// Block height at which deploy allowlist enforcement activates.
    #[serde(default)]
    pub deploy_allowlist_activation_height: Option<u64>,
    /// Fixed admin for dynamic deployment permissions.
    #[serde(default)]
    pub deploy_allowlist_admin: Option<Address>,
}

impl EvolvePayloadBuilderConfig {
    /// Creates a new instance of `EvolvePayloadBuilderConfig`
    pub const fn new() -> Self {
        Self {
            base_fee_sink: None,
            mint_admin: None,
            base_fee_redirect_activation_height: None,
            mint_precompile_activation_height: None,
            proposer_control_admin: None,
            proposer_control_activation_height: None,
            initial_next_proposer: None,
            contract_size_limit: None,
            contract_size_limit_activation_height: None,
            deploy_allowlist: Vec::new(),
            deploy_allowlist_activation_height: None,
            deploy_allowlist_admin: None,
        }
    }

    /// Builds the configuration from the provided chain spec extras.
    pub fn from_chain_spec(spec: &ChainSpec) -> Result<Self, ConfigError> {
        let mut config = Self::default();
        if let Some(extra) = spec
            .genesis
            .config
            .extra_fields
            .get_deserialized::<ChainspecEvolveConfig>("evolve")
        {
            let extras = extra.map_err(ConfigError::InvalidExtras)?;
            config.base_fee_sink = extras.base_fee_sink;
            config.base_fee_redirect_activation_height = extras.base_fee_redirect_activation_height;
            config.mint_admin = extras.mint_admin.filter(|addr| !addr.is_zero());
            config.mint_precompile_activation_height = extras.mint_precompile_activation_height;
            config.proposer_control_admin =
                extras.proposer_control_admin.filter(|addr| !addr.is_zero());
            config.proposer_control_activation_height = extras.proposer_control_activation_height;
            config.initial_next_proposer = extras
                .initial_next_proposer
                .filter(|proposer| !proposer.is_zero());

            if config.base_fee_sink.is_some()
                && config.base_fee_redirect_activation_height.is_none()
            {
                config.base_fee_redirect_activation_height = Some(0);
            }

            if config.mint_admin.is_some() && config.mint_precompile_activation_height.is_none() {
                config.mint_precompile_activation_height = Some(0);
            }

            if config.proposer_control_admin.is_some()
                && config.proposer_control_activation_height.is_none()
            {
                config.proposer_control_activation_height = Some(0);
            }

            config.contract_size_limit = extras.contract_size_limit;
            config.contract_size_limit_activation_height =
                extras.contract_size_limit_activation_height;

            config.deploy_allowlist_admin =
                extras.deploy_allowlist_admin.filter(|addr| !addr.is_zero());

            if let Some(allowlist) = extras.deploy_allowlist {
                config.deploy_allowlist = allowlist;
            }
            config.deploy_allowlist_activation_height = extras.deploy_allowlist_activation_height;
            if config.deploy_allowlist_activation_height.is_none()
                && (config.deploy_allowlist_admin.is_some() || !config.deploy_allowlist.is_empty())
            {
                config.deploy_allowlist_activation_height = Some(0);
            }
        }

        Ok(config)
    }

    /// Returns the contract size limit settings (limit, `activation_height`) if configured.
    /// Returns None if no custom limit is set (uses EIP-170 default).
    pub fn contract_size_limit_settings(&self) -> Option<(usize, u64)> {
        self.contract_size_limit.map(|limit| {
            let activation = self.contract_size_limit_activation_height.unwrap_or(0);
            (limit, activation)
        })
    }

    /// Returns the contract size limit for a given block number.
    /// Uses the custom limit if configured and active, otherwise returns EIP-170 default.
    pub fn contract_size_limit_for_block(&self, block_number: u64) -> usize {
        self.contract_size_limit_settings()
            .and_then(|(limit, activation)| {
                if block_number >= activation {
                    Some(limit)
                } else {
                    None
                }
            })
            .unwrap_or(DEFAULT_CONTRACT_SIZE_LIMIT)
    }

    /// Returns the genesis baseline, activation height, and optional admin.
    pub fn deploy_allowlist_settings(&self) -> Option<DeployAllowlistConfig> {
        if self.deploy_allowlist.is_empty() && self.deploy_allowlist_admin.is_none() {
            None
        } else {
            Some(DeployAllowlistConfig {
                baseline: self.deploy_allowlist.clone(),
                activation_height: self.deploy_allowlist_activation_height.unwrap_or(0),
                admin: self.deploy_allowlist_admin,
            })
        }
    }

    /// Validates the configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.validate_deploy_allowlist()
    }

    fn validate_deploy_allowlist(&self) -> Result<(), ConfigError> {
        let allowlist_len = self.deploy_allowlist.len();
        if allowlist_len > MAX_DEPLOY_ALLOWLIST_LEN {
            return Err(ConfigError::InvalidDeployAllowlist(format!(
                "deployAllowlist has {allowlist_len} entries (max {MAX_DEPLOY_ALLOWLIST_LEN})"
            )));
        }

        let mut seen = HashSet::with_capacity(allowlist_len);
        for addr in &self.deploy_allowlist {
            if addr.is_zero() {
                return Err(ConfigError::InvalidDeployAllowlist(
                    "deployAllowlist contains zero address".to_string(),
                ));
            }
            if !seen.insert(*addr) {
                return Err(ConfigError::InvalidDeployAllowlist(
                    "deployAllowlist contains duplicate entries".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Returns the configured base-fee redirect sink and activation height (defaulting to 0).
    pub fn base_fee_redirect_settings(&self) -> Option<(Address, u64)> {
        self.base_fee_sink.map(|sink| {
            let activation = self.base_fee_redirect_activation_height.unwrap_or(0);
            (sink, activation)
        })
    }

    /// Returns the mint precompile admin and activation height (defaulting to 0).
    pub fn mint_precompile_settings(&self) -> Option<(Address, u64)> {
        self.mint_admin.map(|admin| {
            let activation = self.mint_precompile_activation_height.unwrap_or(0);
            (admin, activation)
        })
    }

    /// Returns the proposer control precompile admin, activation height, and initial proposer.
    pub fn proposer_control_precompile_settings(&self) -> Option<(Address, u64, B256)> {
        self.proposer_control_admin.map(|admin| {
            let activation = self.proposer_control_activation_height.unwrap_or(0);
            let initial_next_proposer = self.initial_next_proposer.unwrap_or(B256::ZERO);
            (admin, activation, initial_next_proposer)
        })
    }

    /// Returns the sink if the redirect is active for the provided block number.
    pub fn base_fee_sink_for_block(&self, block_number: u64) -> Option<Address> {
        self.base_fee_redirect_settings()
            .and_then(|(sink, activation)| (block_number >= activation).then_some(sink))
    }
}

/// Errors that can occur during configuration validation
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Invalid configuration provided
    #[error("Invalid config")]
    InvalidConfig,
    /// Chainspec extras contained invalid values
    #[error("Invalid evolve extras in chainspec: {0}")]
    InvalidExtras(#[from] serde_json::Error),
    /// Deploy allowlist configuration invalid
    #[error("Invalid deploy allowlist configuration: {0}")]
    InvalidDeployAllowlist(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_genesis::Genesis;
    use alloy_primitives::{address, Address, B256};
    use reth_chainspec::ChainSpecBuilder;
    use serde_json::json;

    fn create_test_chainspec_with_extras(extras: Option<serde_json::Value>) -> ChainSpec {
        let mut builder = ChainSpecBuilder::mainnet();

        if let Some(extras_value) = extras {
            // Create a genesis with evolve extras
            let mut genesis = Genesis::default();
            genesis
                .config
                .extra_fields
                .insert("evolve".to_string(), extras_value);
            builder = builder.genesis(genesis);
        }

        builder.build()
    }

    #[test]
    fn test_basefee_sink_some_address() {
        // Test case when base_fee_sink is Some(address)
        let test_address = address!("0000000000000000000000000000000000000001");
        let extras = json!({
            "baseFeeSink": test_address
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.base_fee_sink, Some(test_address));
        assert_eq!(config.mint_admin, None);
        assert_eq!(config.base_fee_redirect_activation_height, Some(0));
        assert_eq!(config.mint_precompile_activation_height, None);
    }

    #[test]
    fn test_mint_admin_some_address() {
        let mint_admin = address!("00000000000000000000000000000000000000aa");
        let extras = json!({
            "mintAdmin": mint_admin
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.base_fee_sink, None);
        assert_eq!(config.mint_admin, Some(mint_admin));
        assert_eq!(config.proposer_control_admin, None);
        assert_eq!(config.base_fee_redirect_activation_height, None);
        assert_eq!(config.mint_precompile_activation_height, Some(0));
        assert_eq!(config.proposer_control_activation_height, None);
    }

    #[test]
    fn test_activation_heights_override() {
        let sink = address!("0000000000000000000000000000000000000002");
        let admin = address!("00000000000000000000000000000000000000bb");
        let extras = json!({
            "baseFeeSink": sink,
            "baseFeeRedirectActivationHeight": 42,
            "mintAdmin": admin,
            "mintPrecompileActivationHeight": 64
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.base_fee_sink, Some(sink));
        assert_eq!(config.base_fee_redirect_activation_height, Some(42));
        assert_eq!(config.mint_admin, Some(admin));
        assert_eq!(config.mint_precompile_activation_height, Some(64));
    }

    #[test]
    fn test_proposer_control_defaults_activation_to_zero() {
        let admin = address!("00000000000000000000000000000000000000cc");
        let initial_next_proposer = B256::from([0xdd; 32]);
        let extras = json!({
            "proposerControlAdmin": admin,
            "initialNextProposer": initial_next_proposer
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.proposer_control_admin, Some(admin));
        assert_eq!(config.proposer_control_activation_height, Some(0));
        assert_eq!(config.initial_next_proposer, Some(initial_next_proposer));
        assert_eq!(
            config.proposer_control_precompile_settings(),
            Some((admin, 0, initial_next_proposer))
        );
    }

    #[test]
    fn test_proposer_control_activation_override_and_zero_initial_ignored() {
        let admin = address!("00000000000000000000000000000000000000cc");
        let extras = json!({
            "proposerControlAdmin": admin,
            "proposerControlActivationHeight": 128,
            "initialNextProposer": "0x0000000000000000000000000000000000000000000000000000000000000000"
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.proposer_control_admin, Some(admin));
        assert_eq!(config.proposer_control_activation_height, Some(128));
        assert_eq!(config.initial_next_proposer, None);
        assert_eq!(
            config.proposer_control_precompile_settings(),
            Some((admin, 128, B256::ZERO))
        );
    }

    #[test]
    fn test_proposer_control_admin_zero_disables() {
        let extras = json!({
            "proposerControlAdmin": "0x0000000000000000000000000000000000000000",
            "initialNextProposer": "0xdddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.proposer_control_admin, None);
        assert_eq!(config.proposer_control_activation_height, None);
        assert!(config.proposer_control_precompile_settings().is_none());
    }

    #[test]
    fn test_mint_admin_zero_disables() {
        let extras = json!({
            "mintAdmin": "0x0000000000000000000000000000000000000000"
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.mint_admin, None);
        assert_eq!(config.mint_precompile_activation_height, None);
    }

    #[test]
    fn test_basefee_sink_none() {
        // Test case when base_fee_sink is not present (None)
        let extras = json!({});

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.base_fee_sink, None);
        assert_eq!(config.base_fee_redirect_activation_height, None);
    }

    #[test]
    fn test_no_ev_reth_extras() {
        // Test case when no evolve extras are present at all
        let chainspec = create_test_chainspec_with_extras(None);
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.base_fee_sink, None);
        assert_eq!(config.mint_admin, None);
        assert_eq!(config.proposer_control_admin, None);
        assert_eq!(config.base_fee_redirect_activation_height, None);
        assert_eq!(config.mint_precompile_activation_height, None);
        assert_eq!(config.proposer_control_activation_height, None);
        assert!(config.deploy_allowlist_settings().is_none());
    }

    #[test]
    fn test_basefee_sink_invalid_address() {
        // Test case when base_fee_sink has invalid format (Error case)
        let extras = json!({
            "baseFeeSink": "not_a_valid_address"
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let result = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::InvalidExtras(_)));
    }

    #[test]
    fn test_basefee_sink_wrong_type() {
        // Test case when base_fee_sink has wrong type (Error case)
        let extras = json!({
            "baseFeeSink": 12345
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let result = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::InvalidExtras(_)));
    }

    #[test]
    fn test_default_config() {
        // Test default configuration
        let config = EvolvePayloadBuilderConfig::default();
        assert_eq!(config.base_fee_sink, None);
        assert_eq!(config.mint_admin, None);
        assert_eq!(config.proposer_control_admin, None);
        assert_eq!(config.base_fee_redirect_activation_height, None);
        assert_eq!(config.mint_precompile_activation_height, None);
        assert_eq!(config.proposer_control_activation_height, None);
        assert!(config.deploy_allowlist.is_empty());
        assert_eq!(config.deploy_allowlist_activation_height, None);
        assert_eq!(config.deploy_allowlist_admin, None);
    }

    #[test]
    fn test_new_config() {
        // Test new() constructor
        let config = EvolvePayloadBuilderConfig::new();
        assert_eq!(config.base_fee_sink, None);
        assert_eq!(config.mint_admin, None);
        assert_eq!(config.proposer_control_admin, None);
        assert_eq!(config.base_fee_redirect_activation_height, None);
        assert_eq!(config.mint_precompile_activation_height, None);
        assert_eq!(config.proposer_control_activation_height, None);
        assert_eq!(config.contract_size_limit, None);
        assert!(config.deploy_allowlist.is_empty());
        assert_eq!(config.deploy_allowlist_activation_height, None);
        assert_eq!(config.deploy_allowlist_admin, None);
    }

    #[test]
    fn test_validate_always_ok() {
        // Test that validate returns Ok for defaults
        let config = EvolvePayloadBuilderConfig::new();
        assert!(config.validate().is_ok());

        let config_with_sink = EvolvePayloadBuilderConfig {
            base_fee_sink: Some(address!("0000000000000000000000000000000000000001")),
            mint_admin: Some(address!("00000000000000000000000000000000000000aa")),
            proposer_control_admin: Some(address!("00000000000000000000000000000000000000bb")),
            base_fee_redirect_activation_height: Some(0),
            mint_precompile_activation_height: Some(0),
            proposer_control_activation_height: Some(0),
            ..Default::default()
        };
        assert!(config_with_sink.validate().is_ok());
    }

    #[test]
    fn test_deploy_allowlist_defaults_activation_to_zero() {
        let allowlist = vec![
            address!("00000000000000000000000000000000000000aa"),
            address!("00000000000000000000000000000000000000bb"),
        ];
        let extras = json!({
            "deployAllowlist": allowlist
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.deploy_allowlist.len(), 2);
        assert_eq!(config.deploy_allowlist_activation_height, Some(0));
    }

    #[test]
    fn test_dynamic_deploy_permissions_default_activation_and_empty_baseline() {
        let admin = address!("00000000000000000000000000000000000000aa");
        let extras = json!({
            "deployAllowlistAdmin": admin
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert!(config.deploy_allowlist.is_empty());
        assert_eq!(config.deploy_allowlist_admin, Some(admin));
        assert_eq!(config.deploy_allowlist_activation_height, Some(0));
        assert_eq!(
            config.deploy_allowlist_settings(),
            Some(DeployAllowlistConfig {
                baseline: Vec::new(),
                activation_height: 0,
                admin: Some(admin),
            })
        );
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_zero_admin_preserves_legacy_mode() {
        let allowed = address!("00000000000000000000000000000000000000aa");
        let extras = json!({
            "deployAllowlist": [allowed],
            "deployAllowlistAdmin": Address::ZERO
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.deploy_allowlist_admin, None);
        assert_eq!(
            config.deploy_allowlist_settings(),
            Some(DeployAllowlistConfig {
                baseline: vec![allowed],
                activation_height: 0,
                admin: None,
            })
        );
    }

    #[test]
    fn test_dynamic_permissions_use_deploy_allowlist_activation_height() {
        let allowed = address!("00000000000000000000000000000000000000aa");
        let admin = address!("00000000000000000000000000000000000000bb");
        let extras = json!({
            "deployAllowlist": [allowed],
            "deployAllowlistActivationHeight": 20,
            "deployAllowlistAdmin": admin
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(
            config.deploy_allowlist_settings(),
            Some(DeployAllowlistConfig {
                baseline: vec![allowed],
                activation_height: 20,
                admin: Some(admin),
            })
        );
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_deploy_allowlist_rejects_zero_address() {
        let extras = json!({
            "deployAllowlist": [
                "0x0000000000000000000000000000000000000000"
            ]
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidDeployAllowlist(_))
        ));
    }

    #[test]
    fn test_deploy_allowlist_rejects_duplicates() {
        let dup = address!("00000000000000000000000000000000000000aa");
        let extras = json!({
            "deployAllowlist": [dup, dup]
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidDeployAllowlist(_))
        ));
    }

    #[test]
    fn test_deploy_allowlist_rejects_too_many_entries() {
        let mut allowlist = Vec::new();
        for i in 0..=MAX_DEPLOY_ALLOWLIST_LEN {
            let mut bytes = [0u8; 20];
            bytes[12..].copy_from_slice(&(i as u64 + 1).to_be_bytes());
            let addr = Address::new(bytes);
            allowlist.push(addr);
        }
        let config = EvolvePayloadBuilderConfig {
            deploy_allowlist: allowlist,
            deploy_allowlist_activation_height: Some(0),
            ..Default::default()
        };

        assert!(matches!(
            config.validate(),
            Err(ConfigError::InvalidDeployAllowlist(_))
        ));
    }

    #[test]
    fn test_base_fee_sink_for_block() {
        let sink = address!("0000000000000000000000000000000000000003");
        let mut config = EvolvePayloadBuilderConfig {
            base_fee_sink: Some(sink),
            base_fee_redirect_activation_height: Some(5),
            ..Default::default()
        };

        assert_eq!(config.base_fee_sink_for_block(4), None);
        assert_eq!(config.base_fee_sink_for_block(5), Some(sink));
        assert_eq!(config.base_fee_sink_for_block(10), Some(sink));

        config.base_fee_redirect_activation_height = None;
        assert_eq!(config.base_fee_sink_for_block(0), Some(sink));
    }

    #[test]
    fn test_chainspec_evolve_config_deserialization() {
        // Test direct deserialization of ChainspecEvolveConfig
        let json_with_sink = json!({
            "baseFeeSink": "0x0000000000000000000000000000000000000001",
            "mintAdmin": "0x00000000000000000000000000000000000000aa"
        });

        let config: ChainspecEvolveConfig = serde_json::from_value(json_with_sink).unwrap();
        assert_eq!(
            config.base_fee_sink,
            Some(address!("0000000000000000000000000000000000000001"))
        );
        assert_eq!(
            config.mint_admin,
            Some(address!("00000000000000000000000000000000000000aa"))
        );

        let json_without_sink = json!({});
        let config: ChainspecEvolveConfig = serde_json::from_value(json_without_sink).unwrap();
        assert_eq!(config.base_fee_sink, None);
        assert_eq!(config.mint_admin, None);
    }

    #[test]
    fn test_contract_size_limit_default() {
        // Test default contract size limit (24KB per EIP-170)
        let config = EvolvePayloadBuilderConfig::new();
        assert_eq!(config.contract_size_limit, None);
        assert_eq!(config.contract_size_limit_settings(), None);
        // When no custom limit is set, use EIP-170 default for any block
        assert_eq!(
            config.contract_size_limit_for_block(0),
            DEFAULT_CONTRACT_SIZE_LIMIT
        );
        assert_eq!(config.contract_size_limit_for_block(0), 24 * 1024);
    }

    #[test]
    fn test_contract_size_limit_from_chainspec() {
        // Test contract size limit from chainspec with activation height
        let extras = json!({
            "contractSizeLimit": 131072,
            "contractSizeLimitActivationHeight": 100
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.contract_size_limit, Some(131072));
        assert_eq!(config.contract_size_limit_activation_height, Some(100));
        assert_eq!(config.contract_size_limit_settings(), Some((131072, 100)));
    }

    #[test]
    fn test_contract_size_limit_respects_activation_height() {
        // Test that contract size limit respects activation height
        let extras = json!({
            "contractSizeLimit": 131072,
            "contractSizeLimitActivationHeight": 100
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        // Before activation: use EIP-170 default
        assert_eq!(
            config.contract_size_limit_for_block(0),
            DEFAULT_CONTRACT_SIZE_LIMIT
        );
        assert_eq!(
            config.contract_size_limit_for_block(99),
            DEFAULT_CONTRACT_SIZE_LIMIT
        );

        // At and after activation: use custom limit
        assert_eq!(config.contract_size_limit_for_block(100), 131072);
        assert_eq!(config.contract_size_limit_for_block(1000), 131072);
    }

    #[test]
    fn test_contract_size_limit_defaults_activation_to_zero() {
        // Test that activation height defaults to 0 when limit is set but height is not
        let extras = json!({
            "contractSizeLimit": 131072
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.contract_size_limit, Some(131072));
        assert_eq!(config.contract_size_limit_activation_height, None);
        // Settings method defaults activation to 0
        assert_eq!(config.contract_size_limit_settings(), Some((131072, 0)));
        // Limit is active from block 0
        assert_eq!(config.contract_size_limit_for_block(0), 131072);
    }

    #[test]
    fn test_contract_size_limit_not_set_uses_default() {
        // Test that missing contractSizeLimit uses EIP-170 default
        let extras = json!({
            "baseFeeSink": "0x0000000000000000000000000000000000000001"
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.contract_size_limit, None);
        assert_eq!(config.contract_size_limit_settings(), None);
        assert_eq!(
            config.contract_size_limit_for_block(0),
            DEFAULT_CONTRACT_SIZE_LIMIT
        );
        assert_eq!(
            config.contract_size_limit_for_block(1000000),
            DEFAULT_CONTRACT_SIZE_LIMIT
        );
    }
}
