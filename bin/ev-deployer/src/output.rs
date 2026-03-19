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

    if let Some(ref mb) = config.contracts.mailbox {
        manifest.insert(
            "mailbox".to_string(),
            Value::String(format!("{}", mb.address)),
        );
    }

    if let Some(ref ni) = config.contracts.noop_ism {
        manifest.insert(
            "noop_ism".to_string(),
            Value::String(format!("{}", ni.address)),
        );
    }

    if let Some(ref p2) = config.contracts.permit2 {
        manifest.insert(
            "permit2".to_string(),
            Value::String(format!("{}", p2.address)),
        );
    }

    if let Some(ref pf) = config.contracts.protocol_fee {
        manifest.insert(
            "protocol_fee".to_string(),
            Value::String(format!("{}", pf.address)),
        );
    }

    Value::Object(manifest)
}
