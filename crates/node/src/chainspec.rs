use alloy_consensus::Header;
use alloy_genesis::{ChainConfig, Genesis};
use alloy_primitives::B256;
use eyre::{bail, eyre, Result, WrapErr};
use reth_chainspec::{BaseFeeParamsKind, ChainSpec, DEV, HOLESKY, HOODI, MAINNET, SEPOLIA};
use reth_cli::chainspec::{parse_genesis, ChainSpecParser};
use reth_primitives_traits::SealedHeader;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tracing::{debug, info, warn};

/// Chains supported by ev-reth. First value should be used as the default.
pub const SUPPORTED_CHAINS: &[&str] = &["mainnet", "sepolia", "holesky", "hoodi", "dev"];

const LIGHTWEIGHT_CHAIN_SPEC_PREFIX: &str = "light:";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct EvolveEip1559Config {
    base_fee_max_change_denominator: Option<u64>,
    base_fee_elasticity_multiplier: Option<u64>,
    initial_base_fee_per_gas: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LightweightChainSpec {
    genesis_hash: B256,
    state_root: B256,
    requires_initialized_datadir: bool,
    genesis: Genesis,
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
        if let Some(input) = s.strip_prefix(LIGHTWEIGHT_CHAIN_SPEC_PREFIX) {
            return parse_lightweight_chain_spec(input);
        }

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
    maybe_write_lightweight_chain_spec(input, &chain_spec);

    Ok(Arc::new(chain_spec))
}

fn parse_lightweight_chain_spec(input: &str) -> Result<Arc<ChainSpec>> {
    let raw = read_chain_spec_input(input).wrap_err("Failed to read lightweight chainspec")?;
    reject_lightweight_alloc(&raw)?;

    let spec: LightweightChainSpec =
        serde_json::from_str(&raw).wrap_err("Failed to parse lightweight chainspec")?;
    validate_lightweight_spec(&spec)?;

    let mut genesis = spec.genesis;
    let overrides = parse_eip1559_overrides(&genesis)?;
    apply_genesis_overrides(&mut genesis, &overrides)?;

    let mut chain_spec: ChainSpec = genesis.into();
    apply_chain_spec_overrides(&mut chain_spec, &overrides)?;
    apply_lightweight_genesis_header(&mut chain_spec, spec.genesis_hash, spec.state_root)?;

    warn!(
        target: "ev-reth::chainspec",
        genesis_hash = ?spec.genesis_hash,
        state_root = ?spec.state_root,
        "Using lightweight chainspec; this requires a datadir already initialized from the full genesis or a trusted snapshot"
    );

    Ok(Arc::new(chain_spec))
}

fn read_chain_spec_input(input: &str) -> Result<String> {
    match fs::read_to_string(input) {
        Ok(raw) => Ok(raw),
        Err(io_err) => {
            if input.contains('{') {
                Ok(input.to_string())
            } else {
                Err(io_err.into())
            }
        }
    }
}

fn maybe_write_lightweight_chain_spec(input: &str, chain_spec: &ChainSpec) {
    let Some(mut path) = lightweight_sidecar_path(input) else {
        debug!(
            target: "ev-reth::chainspec",
            "Skipping lightweight chainspec generation for inline genesis JSON"
        );
        return;
    };

    if let Some(existing_hash) = existing_lightweight_genesis_hash(&path) {
        let genesis_hash = chain_spec.genesis_hash();
        if existing_hash != genesis_hash {
            let alternate_path = lightweight_sidecar_path_with_hash(input, genesis_hash)
                .unwrap_or_else(|| path.clone());
            warn!(
                target: "ev-reth::chainspec",
                path = %path.display(),
                existing_genesis_hash = ?existing_hash,
                generated_genesis_hash = ?genesis_hash,
                alternate_path = %alternate_path.display(),
                "Existing lightweight chainspec points at a different genesis hash; preserving it and writing a hash-specific file"
            );
            path = alternate_path;
        }
    }

    if let Err(err) = write_lightweight_chain_spec(&path, chain_spec) {
        warn!(
            target: "ev-reth::chainspec",
            path = %path.display(),
            error = %err,
            "Failed to generate lightweight chainspec"
        );
    }
}

fn lightweight_sidecar_path(input: &str) -> Option<PathBuf> {
    if input.contains('{') {
        return None;
    }

    let path = Path::new(input);
    let stem = path.file_stem()?.to_string_lossy();
    let mut filename = format!("{stem}.light");
    if let Some(extension) = path.extension() {
        filename.push('.');
        filename.push_str(&extension.to_string_lossy());
    } else {
        filename.push_str(".json");
    }

    Some(path.with_file_name(filename))
}

fn lightweight_sidecar_path_with_hash(input: &str, genesis_hash: B256) -> Option<PathBuf> {
    if input.contains('{') {
        return None;
    }

    let path = Path::new(input);
    let stem = path.file_stem()?.to_string_lossy();
    let hash = format!("{genesis_hash:?}");
    let short_hash = hash
        .trim_start_matches("0x")
        .get(..12)
        .unwrap_or(hash.as_str());
    let mut filename = format!("{stem}.{short_hash}.light");
    if let Some(extension) = path.extension() {
        filename.push('.');
        filename.push_str(&extension.to_string_lossy());
    } else {
        filename.push_str(".json");
    }

    Some(path.with_file_name(filename))
}

fn existing_lightweight_genesis_hash(path: &Path) -> Option<B256> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<LightweightChainSpec>(&raw)
        .map(|spec| spec.genesis_hash)
        .ok()
}

fn write_lightweight_chain_spec(path: &Path, chain_spec: &ChainSpec) -> Result<()> {
    let mut genesis = chain_spec.genesis.clone();
    genesis.alloc.clear();

    let spec = LightweightChainSpec {
        genesis_hash: chain_spec.genesis_hash(),
        state_root: chain_spec.genesis_header().state_root,
        requires_initialized_datadir: true,
        genesis,
    };

    let mut value = serde_json::to_value(&spec).wrap_err("serialize lightweight chainspec")?;
    if let Some(genesis) = value.get_mut("genesis").and_then(Value::as_object_mut) {
        genesis.remove("alloc");
    }
    let raw = serde_json::to_string_pretty(&value).wrap_err("format lightweight chainspec")?;
    fs::write(path, format!("{raw}\n")).wrap_err("write lightweight chainspec")?;

    info!(
        target: "ev-reth::chainspec",
        path = %path.display(),
        "Generated lightweight chainspec for future warm-datadir starts"
    );

    Ok(())
}

fn reject_lightweight_alloc(raw: &str) -> Result<()> {
    let value: Value = serde_json::from_str(raw).wrap_err("Invalid lightweight chainspec JSON")?;
    if value.get("alloc").is_some() || value.pointer("/genesis/alloc").is_some() {
        bail!("lightweight chainspec must not contain alloc; initialize the datadir with the full genesis first");
    }
    Ok(())
}

fn validate_lightweight_spec(spec: &LightweightChainSpec) -> Result<()> {
    if !spec.requires_initialized_datadir {
        bail!("lightweight chainspec requires requiresInitializedDatadir=true");
    }

    if spec.genesis_hash == B256::ZERO {
        bail!("lightweight chainspec requires non-zero genesisHash");
    }

    if spec.state_root == B256::ZERO {
        bail!("lightweight chainspec requires non-zero stateRoot");
    }

    if spec.genesis.config.chain_id == 0 {
        bail!("lightweight chainspec requires non-zero config.chainId");
    }

    if spec.genesis.number.unwrap_or_default() != 0 {
        bail!("lightweight chainspec genesis number must be 0");
    }

    validate_lightweight_fork_config(&spec.genesis.config)?;

    Ok(())
}

fn validate_lightweight_fork_config(config: &ChainConfig) -> Result<()> {
    if config.eip155_block.is_none() {
        bail!("lightweight chainspec requires config.eip155Block");
    }

    if config.eip158_block.is_none() {
        bail!("lightweight chainspec requires config.eip158Block");
    }

    if config.london_block == Some(0) && config.terminal_total_difficulty.is_none() {
        bail!(
            "lightweight chainspec with London at genesis requires config.terminalTotalDifficulty"
        );
    }

    Ok(())
}

fn apply_lightweight_genesis_header(
    chain_spec: &mut ChainSpec,
    genesis_hash: B256,
    state_root: B256,
) -> Result<()> {
    let mut header: Header = chain_spec.genesis_header().clone();
    header.state_root = state_root;
    header.number = chain_spec.genesis.number.unwrap_or_default();

    // Light mode is for opening an already-initialized datadir, so trust the
    // supplied DB genesis hash instead of requiring alloc-derived header parity.
    chain_spec.genesis_header = SealedHeader::new(header, genesis_hash);
    Ok(())
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
    use crate::config::EvolvePayloadBuilderConfig;
    use alloy_genesis::Genesis;
    use alloy_primitives::U256;
    use serde_json::{json, Value};

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

    fn base_lightweight_genesis() -> Genesis {
        let mut genesis = Genesis::default();
        genesis.config.chain_id = 1234;
        genesis.config.eip155_block = Some(0);
        genesis.config.eip158_block = Some(0);
        genesis.config.london_block = Some(0);
        genesis.config.terminal_total_difficulty = Some(U256::ZERO);
        genesis.gas_limit = 30_000_000;
        genesis
    }

    fn lightweight_json(genesis: Genesis) -> Value {
        let chain_spec: ChainSpec = genesis.clone().into();
        let header = chain_spec.genesis_header();
        let mut value = json!({
            "genesisHash": chain_spec.genesis_hash(),
            "stateRoot": header.state_root,
            "requiresInitializedDatadir": true,
            "genesis": genesis,
        });
        value["genesis"].as_object_mut().unwrap().remove("alloc");
        value
    }

    #[test]
    fn test_lightweight_chainspec_parses_without_alloc() {
        let raw = lightweight_json(base_lightweight_genesis()).to_string();
        let chain_spec =
            EvolveChainSpecParser::parse(&format!("{LIGHTWEIGHT_CHAIN_SPEC_PREFIX}{raw}")).unwrap();

        assert_eq!(chain_spec.chain().id(), 1234);
        assert!(chain_spec.genesis.alloc.is_empty());
        assert_eq!(chain_spec.genesis_hash(), chain_spec.genesis_header.hash());
    }

    #[test]
    fn test_lightweight_chainspec_rejects_alloc() {
        let mut value = lightweight_json(base_lightweight_genesis());
        value["genesis"]["alloc"] = json!({});

        let err = EvolveChainSpecParser::parse(&format!("{LIGHTWEIGHT_CHAIN_SPEC_PREFIX}{value}"))
            .unwrap_err();

        assert!(err.to_string().contains("must not contain alloc"));
    }

    #[test]
    fn test_lightweight_chainspec_requires_identity_fields() {
        let mut value = lightweight_json(base_lightweight_genesis());
        value.as_object_mut().unwrap().remove("genesisHash");

        let err = EvolveChainSpecParser::parse(&format!("{LIGHTWEIGHT_CHAIN_SPEC_PREFIX}{value}"))
            .unwrap_err();

        assert!(format!("{err:?}").contains("Failed to parse lightweight chainspec"));
    }

    #[test]
    fn test_lightweight_chainspec_trusts_supplied_genesis_hash() {
        let mut value = lightweight_json(base_lightweight_genesis());
        let supplied_hash = B256::from_slice(&[0x11; 32]);
        value["genesisHash"] = serde_json::to_value(supplied_hash).unwrap();

        let chain_spec =
            EvolveChainSpecParser::parse(&format!("{LIGHTWEIGHT_CHAIN_SPEC_PREFIX}{value}"))
                .unwrap();

        assert_eq!(chain_spec.genesis_hash(), supplied_hash);
    }

    #[test]
    fn test_lightweight_chainspec_preserves_evolve_extras() {
        let mut genesis = base_lightweight_genesis();
        genesis
            .config
            .extra_fields
            .insert_value(
                "evolve".to_string(),
                json!({
                    "baseFeeSink": "0x00000000000000000000000000000000000000fe",
                    "baseFeeRedirectActivationHeight": 7,
                    "baseFeeMaxChangeDenominator": 10,
                    "baseFeeElasticityMultiplier": 4,
                    "initialBaseFeePerGas": 7
                }),
            )
            .unwrap();

        let mut value = lightweight_json(genesis);
        let overridden_genesis: Genesis = serde_json::from_value(value["genesis"].clone()).unwrap();
        let overridden_hash = apply_overrides(&overridden_genesis).unwrap().genesis_hash();
        value["genesisHash"] = serde_json::to_value(overridden_hash).unwrap();

        let chain_spec =
            EvolveChainSpecParser::parse(&format!("{LIGHTWEIGHT_CHAIN_SPEC_PREFIX}{value}"))
                .unwrap();
        let config = EvolvePayloadBuilderConfig::from_chain_spec(chain_spec.as_ref()).unwrap();

        assert_eq!(config.base_fee_redirect_activation_height, Some(7));
        assert_eq!(chain_spec.genesis.base_fee_per_gas, Some(7));
        let params = chain_spec.base_fee_params_at_timestamp(chain_spec.genesis.timestamp);
        assert_eq!(params.max_change_denominator, 10);
        assert_eq!(params.elasticity_multiplier, 4);
    }

    #[test]
    fn test_custom_chainspec_generates_lightweight_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let genesis_path = dir.path().join("genesis.json");
        std::fs::write(
            &genesis_path,
            serde_json::to_string(&base_lightweight_genesis()).unwrap(),
        )
        .unwrap();

        let full_spec = parse_custom_chain_spec(genesis_path.to_str().unwrap()).unwrap();
        let light_path = dir.path().join("genesis.light.json");
        let raw = std::fs::read_to_string(&light_path).unwrap();

        assert!(!raw.contains("\"alloc\""));

        let light_spec =
            EvolveChainSpecParser::parse(&format!("light:{}", light_path.display())).unwrap();

        assert_eq!(light_spec.genesis_hash(), full_spec.genesis_hash());
        assert_eq!(
            light_spec.genesis_header().state_root,
            full_spec.genesis_header().state_root
        );
        assert!(light_spec.genesis.alloc.is_empty());
    }

    #[test]
    fn test_custom_chainspec_preserves_existing_sidecar_for_different_hash() {
        let dir = tempfile::tempdir().unwrap();
        let genesis_path = dir.path().join("genesis.json");
        let light_path = dir.path().join("genesis.light.json");
        let old_value = lightweight_json(base_lightweight_genesis());
        std::fs::write(
            &light_path,
            serde_json::to_string_pretty(&old_value).unwrap(),
        )
        .unwrap();

        let mut new_genesis = base_lightweight_genesis();
        new_genesis.timestamp = 1;
        std::fs::write(&genesis_path, serde_json::to_string(&new_genesis).unwrap()).unwrap();

        let full_spec = parse_custom_chain_spec(genesis_path.to_str().unwrap()).unwrap();
        let preserved: LightweightChainSpec =
            serde_json::from_str(&std::fs::read_to_string(&light_path).unwrap()).unwrap();

        assert_ne!(preserved.genesis_hash, full_spec.genesis_hash());

        let hash = format!("{:?}", full_spec.genesis_hash());
        let short_hash = hash.trim_start_matches("0x").get(..12).unwrap();
        let alternate_path = dir.path().join(format!("genesis.{short_hash}.light.json"));
        assert!(alternate_path.exists());
    }
}
