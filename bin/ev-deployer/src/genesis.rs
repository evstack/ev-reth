//! Genesis alloc JSON builder.

use crate::{config::DeployConfig, contracts::GenesisContract};
use alloy_primitives::B256;
use serde_json::{Map, Value};
use std::path::Path;

/// Build the alloc JSON from config.
pub(crate) fn build_alloc(_config: &DeployConfig) -> Value {
    let alloc = Map::new();
    Value::Object(alloc)
}

/// Build alloc and merge into an existing genesis JSON file.
pub(crate) fn merge_into(
    config: &DeployConfig,
    genesis_path: &Path,
    force: bool,
) -> eyre::Result<Value> {
    let content = std::fs::read_to_string(genesis_path)?;
    let mut genesis: Value = serde_json::from_str(&content)?;

    let alloc = build_alloc(config);

    let genesis_alloc = genesis
        .get_mut("alloc")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| eyre::eyre!("genesis JSON missing 'alloc' object"))?;

    let new_alloc = alloc.as_object().unwrap();
    for (addr, entry) in new_alloc {
        if genesis_alloc.contains_key(addr) && !force {
            eyre::bail!("address collision at {addr}; use --force to overwrite");
        }
        genesis_alloc.insert(addr.clone(), entry.clone());
    }

    Ok(genesis)
}

#[allow(dead_code)]
fn insert_contract(alloc: &mut Map<String, Value>, contract: &GenesisContract) {
    // Address key without 0x prefix, using checksummed format
    let addr_hex = format!("{}", contract.address);
    let addr_key = addr_hex.strip_prefix("0x").unwrap_or(&addr_hex);

    let mut storage_map = Map::new();
    for (slot, value) in &contract.storage {
        let slot_key = format_slot_key(slot);
        storage_map.insert(slot_key, Value::String(format!("{value}")));
    }

    let mut entry = Map::new();
    entry.insert("balance".to_string(), Value::String("0x0".to_string()));
    entry.insert(
        "code".to_string(),
        Value::String(format!(
            "0x{}",
            alloy_primitives::hex::encode(&contract.code)
        )),
    );
    entry.insert("storage".to_string(), Value::Object(storage_map));

    alloc.insert(addr_key.to_string(), Value::Object(entry));
}

/// Format a storage slot key as a full 32-byte hex string.
/// `B256::ZERO` -> "0x0000000000000000000000000000000000000000000000000000000000000000"
#[allow(dead_code)]
fn format_slot_key(slot: &B256) -> String {
    format!("{slot}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;

    fn test_config() -> DeployConfig {
        DeployConfig {
            chain: ChainConfig { chain_id: 1234 },
            contracts: ContractsConfig {},
        }
    }

    #[test]
    fn empty_alloc() {
        let alloc = build_alloc(&test_config());
        let obj = alloc.as_object().unwrap();
        assert!(obj.is_empty());
    }

    #[test]
    fn slot_key_formatting() {
        assert_eq!(
            format_slot_key(&B256::ZERO),
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        );
        assert_eq!(
            format_slot_key(&B256::with_last_byte(1)),
            "0x0000000000000000000000000000000000000000000000000000000000000001"
        );
        assert_eq!(
            format_slot_key(&B256::with_last_byte(6)),
            "0x0000000000000000000000000000000000000000000000000000000000000006"
        );
    }

    #[test]
    fn merge_into_existing_genesis() {
        let genesis = r#"{"alloc":{"deadbeef":{"balance":"0x1"}}}"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), genesis).unwrap();

        let result = merge_into(&test_config(), tmp.path(), false).unwrap();
        let alloc = result.get("alloc").unwrap().as_object().unwrap();
        assert!(alloc.contains_key("deadbeef"));
    }
}
