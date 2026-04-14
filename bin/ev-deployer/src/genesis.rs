//! Genesis alloc JSON builder.

use crate::{
    config::DeployConfig,
    contracts::{self, GenesisContract},
};
use alloy_primitives::B256;
use serde_json::{Map, Value};
use std::path::Path;

/// Build the alloc JSON from config.
pub(crate) fn build_alloc(config: &DeployConfig) -> Value {
    let mut alloc = Map::new();

    if let Some(ref ap_config) = config.contracts.admin_proxy {
        let contract = contracts::admin_proxy::build(ap_config);
        insert_contract(&mut alloc, &contract);
    }

    if let Some(ref p2_config) = config.contracts.permit2 {
        let contract = contracts::permit2::build(p2_config, config.chain.chain_id);
        insert_contract(&mut alloc, &contract);
    }

    if let Some(ref dd_config) = config.contracts.deterministic_deployer {
        let contract = contracts::deterministic_deployer::build(dd_config);
        insert_contract(&mut alloc, &contract);
    }

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
        let canonical = normalize_addr(addr);
        let existing_key = genesis_alloc
            .keys()
            .find(|k| normalize_addr(k) == canonical)
            .cloned();
        if existing_key.is_some() && !force {
            eyre::bail!("address collision at {addr}; use --force to overwrite");
        }
        if let Some(key) = existing_key {
            genesis_alloc.remove(&key);
        }
        genesis_alloc.insert(canonical, entry.clone());
    }

    Ok(genesis)
}

fn normalize_addr(addr: &str) -> String {
    addr.strip_prefix("0x").unwrap_or(addr).to_lowercase()
}

fn insert_contract(alloc: &mut Map<String, Value>, contract: &GenesisContract) {
    let addr_key = normalize_addr(&format!("{}", contract.address));

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

    alloc.insert(addr_key, Value::Object(entry));
}

/// Format a storage slot key as a full 32-byte hex string.
/// `B256::ZERO` -> "0x0000000000000000000000000000000000000000000000000000000000000000"
fn format_slot_key(slot: &B256) -> String {
    format!("{slot}")
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
                    address: Some(address!("000000000000000000000000000000000000ad00")),
                    owner: address!("f39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
                }),
                permit2: None,
                deterministic_deployer: None,
            },
        }
    }

    #[test]
    fn alloc_json_structure() {
        let alloc = build_alloc(&test_config());
        let obj = alloc.as_object().unwrap();
        assert!(obj.contains_key("000000000000000000000000000000000000ad00"));

        let entry = obj
            .get("000000000000000000000000000000000000ad00")
            .unwrap()
            .as_object()
            .unwrap();
        assert_eq!(entry["balance"], "0x0");
        assert!(entry["code"].as_str().unwrap().starts_with("0x"));
        assert!(entry.contains_key("storage"));
    }

    #[test]
    fn alloc_golden_value() {
        let alloc = build_alloc(&test_config());
        let storage = alloc
            .as_object()
            .unwrap()
            .get("000000000000000000000000000000000000ad00")
            .unwrap()
            .get("storage")
            .unwrap()
            .as_object()
            .unwrap();

        assert_eq!(
            storage["0x0000000000000000000000000000000000000000000000000000000000000000"],
            "0x000000000000000000000000f39fd6e51aad88f6f4ce6ab8827279cfffb92266"
        );
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
    fn merge_detects_collision() {
        let genesis = r#"{"alloc":{"000000000000000000000000000000000000ad00":{"balance":"0x0"}}}"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), genesis).unwrap();

        let result = merge_into(&test_config(), tmp.path(), false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("address collision"));
    }

    #[test]
    fn merge_force_overwrites() {
        let genesis = r#"{"alloc":{"000000000000000000000000000000000000ad00":{"balance":"0x0"}}}"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), genesis).unwrap();

        let result = merge_into(&test_config(), tmp.path(), true);
        assert!(result.is_ok());
    }

    #[test]
    fn merge_detects_collision_with_0x_prefix() {
        let genesis =
            r#"{"alloc":{"0x000000000000000000000000000000000000ad00":{"balance":"0x0"}}}"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), genesis).unwrap();

        let result = merge_into(&test_config(), tmp.path(), false);
        assert!(result.is_err());
    }

    #[test]
    fn merge_detects_collision_with_mixed_case() {
        let genesis = r#"{"alloc":{"000000000000000000000000000000000000AD00":{"balance":"0x0"}}}"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), genesis).unwrap();

        let result = merge_into(&test_config(), tmp.path(), false);
        assert!(result.is_err());
    }

    #[test]
    fn deterministic_deployer_in_alloc() {
        let config = DeployConfig {
            chain: ChainConfig { chain_id: 1234 },
            contracts: ContractsConfig {
                admin_proxy: None,
                permit2: None,
                deterministic_deployer: Some(DeterministicDeployerConfig {
                    address: Some(address!("4e59b44847b379578588920cA78FbF26c0B4956C")),
                }),
            },
        };
        let alloc = build_alloc(&config);
        let obj = alloc.as_object().unwrap();
        let key = "4e59b44847b379578588920ca78fbf26c0b4956c";
        assert!(obj.contains_key(key), "missing key {key} in {obj:?}");

        let entry = obj.get(key).unwrap().as_object().unwrap();
        assert_eq!(entry["balance"], "0x0");
        assert!(entry["code"].as_str().unwrap().starts_with("0x"));
        assert!(entry["storage"].as_object().unwrap().is_empty());
    }
}
