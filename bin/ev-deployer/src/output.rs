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

    Value::Object(manifest)
}
