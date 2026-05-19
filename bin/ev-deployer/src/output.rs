//! Address manifest output.

use crate::config::DeployConfig;
use serde_json::{Map, Value};

/// Build an address manifest JSON from config.
pub fn build_manifest(config: &DeployConfig) -> Value {
    let mut manifest = Map::new();

    if let Some(ref ap) = config.contracts.admin_proxy {
        if let Some(addr) = ap.address {
            manifest.insert(
                "admin_proxy".to_string(),
                Value::String(format!("{}", addr)),
            );
        }
    }

    if let Some(ref p2) = config.contracts.permit2 {
        if let Some(addr) = p2.address {
            manifest.insert("permit2".to_string(), Value::String(format!("{}", addr)));
        }
    }

    if let Some(ref dd) = config.contracts.deterministic_deployer {
        if let Some(addr) = dd.address {
            manifest.insert(
                "deterministic_deployer".to_string(),
                Value::String(format!("{}", addr)),
            );
        }
    }

    Value::Object(manifest)
}
