use alloy_primitives::{Address, U256};
use reth_chainspec::ChainSpec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ChainspecEvolveConfig {
    #[serde(default, rename = "baseFeeSink")]
    pub base_fee_sink: Option<Address>,
    #[serde(default, rename = "mintAdmin")]
    pub mint_admin: Option<Address>,
    #[serde(default, rename = "tokenDualityAdmin")]
    pub token_duality_admin: Option<Address>,
    #[serde(default, rename = "tokenDualityPerCallCap")]
    pub token_duality_per_call_cap: Option<U256>,
    #[serde(default, rename = "tokenDualityPerBlockCap")]
    pub token_duality_per_block_cap: Option<U256>,
}

/// Configuration for the Evolve payload builder
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvolvePayloadBuilderConfig {
    /// Optional chainspec-configured recipient for redirected base fees.
    #[serde(default)]
    pub base_fee_sink: Option<Address>,
    /// Optional mint precompile admin address sourced from the chainspec.
    #[serde(default)]
    pub mint_admin: Option<Address>,
    /// Optional token duality precompile admin address sourced from the chainspec.
    #[serde(default)]
    pub token_duality_admin: Option<Address>,
    /// Optional per-call transfer cap for token duality precompile.
    #[serde(default)]
    pub token_duality_per_call_cap: Option<U256>,
    /// Optional per-block transfer cap for token duality precompile.
    #[serde(default)]
    pub token_duality_per_block_cap: Option<U256>,
}

impl EvolvePayloadBuilderConfig {
    /// Creates a new instance of `EvolvePayloadBuilderConfig`
    pub const fn new() -> Self {
        Self {
            base_fee_sink: None,
            mint_admin: None,
            token_duality_admin: None,
            token_duality_per_call_cap: None,
            token_duality_per_block_cap: None,
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
            config.mint_admin =
                extras
                    .mint_admin
                    .and_then(|addr| if addr.is_zero() { None } else { Some(addr) });
            config.token_duality_admin =
                extras
                    .token_duality_admin
                    .and_then(|addr| if addr.is_zero() { None } else { Some(addr) });
            config.token_duality_per_call_cap = extras.token_duality_per_call_cap;
            config.token_duality_per_block_cap = extras.token_duality_per_block_cap;
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
    #[error("Invalid evolve extras in chainspec: {0}")]
    InvalidExtras(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_genesis::Genesis;
    use alloy_primitives::address;
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
    }

    #[test]
    fn test_mint_admin_zero_disables() {
        let extras = json!({
            "mintAdmin": "0x0000000000000000000000000000000000000000"
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.mint_admin, None);
    }

    #[test]
    fn test_basefee_sink_none() {
        // Test case when base_fee_sink is not present (None)
        let extras = json!({});

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.base_fee_sink, None);
    }

    #[test]
    fn test_no_ev_reth_extras() {
        // Test case when no evolve extras are present at all
        let chainspec = create_test_chainspec_with_extras(None);
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.base_fee_sink, None);
        assert_eq!(config.mint_admin, None);
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
        assert_eq!(config.token_duality_admin, None);
        assert_eq!(config.token_duality_per_call_cap, None);
        assert_eq!(config.token_duality_per_block_cap, None);
    }

    #[test]
    fn test_new_config() {
        // Test new() constructor
        let config = EvolvePayloadBuilderConfig::new();
        assert_eq!(config.base_fee_sink, None);
        assert_eq!(config.mint_admin, None);
        assert_eq!(config.token_duality_admin, None);
        assert_eq!(config.token_duality_per_call_cap, None);
        assert_eq!(config.token_duality_per_block_cap, None);
    }

    #[test]
    fn test_validate_always_ok() {
        // Test that validate always returns Ok for now
        let config = EvolvePayloadBuilderConfig::new();
        assert!(config.validate().is_ok());

        let config_with_sink = EvolvePayloadBuilderConfig {
            base_fee_sink: Some(address!("0000000000000000000000000000000000000001")),
            mint_admin: Some(address!("00000000000000000000000000000000000000aa")),
            token_duality_admin: Some(address!("00000000000000000000000000000000000000bb")),
            token_duality_per_call_cap: Some(U256::from(1000)),
            token_duality_per_block_cap: Some(U256::from(10000)),
        };
        assert!(config_with_sink.validate().is_ok());
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
    fn test_token_duality_admin_some_address() {
        let token_duality_admin = address!("00000000000000000000000000000000000000fd");
        let extras = json!({
            "tokenDualityAdmin": token_duality_admin
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.token_duality_admin, Some(token_duality_admin));
        assert_eq!(config.token_duality_per_call_cap, None);
        assert_eq!(config.token_duality_per_block_cap, None);
    }

    #[test]
    fn test_token_duality_admin_zero_disables() {
        let extras = json!({
            "tokenDualityAdmin": "0x0000000000000000000000000000000000000000"
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.token_duality_admin, None);
    }

    #[test]
    fn test_token_duality_with_caps() {
        let token_duality_admin = address!("00000000000000000000000000000000000000fd");
        let extras = json!({
            "tokenDualityAdmin": token_duality_admin,
            "tokenDualityPerCallCap": "0x3e8",
            "tokenDualityPerBlockCap": "0x2710"
        });

        let chainspec = create_test_chainspec_with_extras(Some(extras));
        let config = EvolvePayloadBuilderConfig::from_chain_spec(&chainspec).unwrap();

        assert_eq!(config.token_duality_admin, Some(token_duality_admin));
        assert_eq!(config.token_duality_per_call_cap, Some(U256::from(1000)));
        assert_eq!(config.token_duality_per_block_cap, Some(U256::from(10000)));
    }

    #[test]
    fn test_full_config_deserialization() {
        let json_full = json!({
            "baseFeeSink": "0x0000000000000000000000000000000000000001",
            "mintAdmin": "0x00000000000000000000000000000000000000aa",
            "tokenDualityAdmin": "0x00000000000000000000000000000000000000fd",
            "tokenDualityPerCallCap": "0x64",
            "tokenDualityPerBlockCap": "0x3e8"
        });

        let config: ChainspecEvolveConfig = serde_json::from_value(json_full).unwrap();
        assert_eq!(
            config.base_fee_sink,
            Some(address!("0000000000000000000000000000000000000001"))
        );
        assert_eq!(
            config.mint_admin,
            Some(address!("00000000000000000000000000000000000000aa"))
        );
        assert_eq!(
            config.token_duality_admin,
            Some(address!("00000000000000000000000000000000000000fd"))
        );
        assert_eq!(config.token_duality_per_call_cap, Some(U256::from(100)));
        assert_eq!(config.token_duality_per_block_cap, Some(U256::from(1000)));
    }
}
