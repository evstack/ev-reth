//! Address manifest output.

use crate::config::DeployConfig;
use serde_json::{Map, Value};

/// Build an address manifest JSON from config.
pub fn build_manifest(config: &DeployConfig) -> Value {
    let mut manifest = Map::new();

    if let Some(ref ap) = config.contracts.admin_proxy {
        manifest.insert(
            "admin_proxy".to_string(),
            Value::String(format!("{}", ap.address)),
        );
    }

    if let Some(ref p2) = config.contracts.permit2 {
        manifest.insert(
            "permit2".to_string(),
            Value::String(format!("{}", p2.address)),
        );
    }

    Value::Object(manifest)
}
