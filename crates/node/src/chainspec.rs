use alloy_genesis::Genesis;
use eyre::{bail, eyre, Result, WrapErr};
use reth_chainspec::{BaseFeeParamsKind, ChainSpec, DEV, HOLESKY, HOODI, MAINNET, SEPOLIA};
use reth_cli::chainspec::{parse_genesis, ChainSpecParser};
use serde::Deserialize;
use std::sync::Arc;

/// Chains supported by ev-reth. First value should be used as the default.
pub const SUPPORTED_CHAINS: &[&str] = &["mainnet", "sepolia", "holesky", "hoodi", "dev"];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct EvolveEip1559Config {
    base_fee_max_change_denominator: Option<u64>,
    base_fee_elasticity_multiplier: Option<u64>,
    initial_base_fee_per_gas: Option<u64>,
}

impl EvolveEip1559Config {
    const fn has_base_fee_overrides(&self) -> bool {
        self.base_fee_max_change_denominator.is_some()
            || self.base_fee_elasticity_multiplier.is_some()
    }
}

/// Chainspec parser that applies ev-reth specific EIP-1559 overrides from the evolve extras block.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct EvolveChainSpecParser;

impl ChainSpecParser for EvolveChainSpecParser {
    type ChainSpec = ChainSpec;

    const SUPPORTED_CHAINS: &'static [&'static str] = SUPPORTED_CHAINS;

    fn parse(s: &str) -> Result<Arc<ChainSpec>> {
        match s {
            "mainnet" => Ok(MAINNET.clone()),
            "sepolia" => Ok(SEPOLIA.clone()),
            "holesky" => Ok(HOLESKY.clone()),
            "hoodi" => Ok(HOODI.clone()),
            "dev" => Ok(DEV.clone()),
            _ => parse_custom_chain_spec(s),
        }
    }
}

fn parse_custom_chain_spec(input: &str) -> Result<Arc<ChainSpec>> {
    let mut genesis = parse_genesis(input).wrap_err("Failed to parse genesis config")?;
    let overrides = parse_eip1559_overrides(&genesis)?;
    apply_genesis_overrides(&mut genesis, &overrides)?;

    let mut chain_spec: ChainSpec = genesis.into();
    apply_chain_spec_overrides(&mut chain_spec, &overrides)?;

    Ok(Arc::new(chain_spec))
}

fn parse_eip1559_overrides(genesis: &Genesis) -> Result<EvolveEip1559Config> {
    match genesis
        .config
        .extra_fields
        .get_deserialized::<EvolveEip1559Config>("evolve")
    {
        Some(Ok(config)) => Ok(config),
        Some(Err(err)) => Err(eyre!(err)).wrap_err("Invalid evolve extras in chainspec"),
        None => Ok(EvolveEip1559Config::default()),
    }
}

fn apply_genesis_overrides(genesis: &mut Genesis, overrides: &EvolveEip1559Config) -> Result<()> {
    let Some(initial_base_fee) = overrides.initial_base_fee_per_gas else {
        return Ok(());
    };

    if genesis.config.london_block != Some(0) {
        bail!("initialBaseFeePerGas requires londonBlock set to 0 in the chainspec config");
    }

    let initial_base_fee_u128 = u128::from(initial_base_fee);
    if let Some(existing) = genesis.base_fee_per_gas {
        if existing != initial_base_fee_u128 {
            bail!(
                "initialBaseFeePerGas conflicts with baseFeePerGas in genesis ({} != {})",
                initial_base_fee_u128,
                existing
            );
        }
    }

    genesis.base_fee_per_gas = Some(initial_base_fee_u128);
    Ok(())
}

fn apply_chain_spec_overrides(
    chain_spec: &mut ChainSpec,
    overrides: &EvolveEip1559Config,
) -> Result<()> {
    if let Some(denominator) = overrides.base_fee_max_change_denominator {
        if denominator == 0 {
            bail!(
                "baseFeeMaxChangeDenominator must be greater than 0, got: {}",
                denominator
            );
        }
    }

    if let Some(elasticity) = overrides.base_fee_elasticity_multiplier {
        if elasticity == 0 {
            bail!(
                "baseFeeElasticityMultiplier must be greater than 0, got: {}",
                elasticity
            );
        }
    }

    if !overrides.has_base_fee_overrides() {
        return Ok(());
    }

    let mut params = chain_spec.base_fee_params_at_timestamp(chain_spec.genesis.timestamp);

    if let Some(denominator) = overrides.base_fee_max_change_denominator {
        params.max_change_denominator = u128::from(denominator);
    }

    if let Some(elasticity) = overrides.base_fee_elasticity_multiplier {
        params.elasticity_multiplier = u128::from(elasticity);
    }

    chain_spec.base_fee_params = BaseFeeParamsKind::Constant(params);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_genesis::Genesis;
    use serde_json::json;

    fn apply_overrides(genesis: &Genesis) -> Result<ChainSpec> {
        let overrides = parse_eip1559_overrides(genesis)?;
        let mut genesis = genesis.clone();
        apply_genesis_overrides(&mut genesis, &overrides)?;
        let mut chain_spec: ChainSpec = genesis.into();
        apply_chain_spec_overrides(&mut chain_spec, &overrides)?;
        Ok(chain_spec)
    }

    #[test]
    fn test_eip1559_overrides_apply() {
        let mut genesis = Genesis::default();
        genesis.config.chain_id = 1;
        genesis.config.london_block = Some(0);
        genesis
            .config
            .extra_fields
            .insert_value(
                "evolve".to_string(),
                json!({
                    "baseFeeMaxChangeDenominator": 10,
                    "baseFeeElasticityMultiplier": 4,
                    "initialBaseFeePerGas": 7
                }),
            )
            .unwrap();

        let chain_spec = apply_overrides(&genesis).unwrap();
        let params = chain_spec.base_fee_params_at_timestamp(chain_spec.genesis.timestamp);
        assert_eq!(params.max_change_denominator, 10);
        assert_eq!(params.elasticity_multiplier, 4);
        assert_eq!(chain_spec.genesis.base_fee_per_gas, Some(7));
    }

    #[test]
    fn test_initial_base_fee_requires_london_genesis() {
        let mut genesis = Genesis::default();
        genesis.config.chain_id = 1;
        genesis.config.london_block = Some(10);
        genesis
            .config
            .extra_fields
            .insert_value(
                "evolve".to_string(),
                json!({
                    "initialBaseFeePerGas": 7
                }),
            )
            .unwrap();

        let err = apply_overrides(&genesis).unwrap_err();
        assert!(err
            .to_string()
            .contains("initialBaseFeePerGas requires londonBlock set to 0"));
    }

    #[test]
    fn test_no_overrides_preserves_defaults() {
        let mut genesis = Genesis::default();
        genesis.config.chain_id = 1;
        genesis.config.london_block = Some(0);
        // No evolve config at all

        let chain_spec = apply_overrides(&genesis).unwrap();
        let params = chain_spec.base_fee_params_at_timestamp(chain_spec.genesis.timestamp);
        // Should be Ethereum mainnet defaults
        assert_eq!(params.max_change_denominator, 8);
        assert_eq!(params.elasticity_multiplier, 2);
    }

    #[test]
    fn test_base_fee_denominator_must_be_positive() {
        let mut genesis = Genesis::default();
        genesis.config.chain_id = 1;
        genesis.config.london_block = Some(0);
        genesis
            .config
            .extra_fields
            .insert_value(
                "evolve".to_string(),
                json!({
                    "baseFeeMaxChangeDenominator": 0
                }),
            )
            .unwrap();

        let err = apply_overrides(&genesis).unwrap_err();
        assert!(err
            .to_string()
            .contains("baseFeeMaxChangeDenominator must be greater than 0"));
    }
}
