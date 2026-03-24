//! Address manifest output.

use crate::config::DeployConfig;
use serde_json::{Map, Value};

/// Build an address manifest JSON from config.
pub(crate) fn build_manifest(_config: &DeployConfig) -> Value {
    let manifest = Map::new();
    Value::Object(manifest)
}
