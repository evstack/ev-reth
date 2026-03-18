//! Address manifest output.

use crate::config::DeployConfig;
use serde_json::{Map, Value};

/// Build an address manifest JSON from config.
pub(crate) fn build_manifest(config: &DeployConfig) -> Value {
    let mut manifest = Map::new();

    if let Some(ref ap) = config.contracts.admin_proxy {
        manifest.insert(
            "admin_proxy".to_string(),
            Value::String(format!("{}", ap.address)),
        );
    }

    if let Some(ref fv) = config.contracts.fee_vault {
        manifest.insert(
            "fee_vault".to_string(),
            Value::String(format!("{}", fv.address)),
        );
    }

    if let Some(ref mth) = config.contracts.merkle_tree_hook {
        manifest.insert(
            "merkle_tree_hook".to_string(),
            Value::String(format!("{}", mth.address)),
        );
    }

    Value::Object(manifest)
}
