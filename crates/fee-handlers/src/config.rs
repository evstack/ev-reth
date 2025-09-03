use crate::types::*;
use alloy_primitives::Address;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing feeHandlers config in chainspec extras")]
    Missing,
    #[error("invalid config: {0}")]
    Invalid(String),
}

/// Reads `ev_reth.feeHandlers` from a chainspec/extras JSON blob.
///
/// Expected shape (example):
/// {
///   "ev_reth": {
///     "feeHandlers": {
///       "vaults": {
///         "sequencer_fee_vault": "0x4200...011",
///         "base_fee_vault": "0x4200...019",
///         "l1_fee_vault": "0x4200...01a",
///         "operator_fee_vault": "0x4200...01b"
///       },
///       "l1_params": { "mode": "Ecotone", "ecotone": { "base_fee_scalar": 1000000, "blob_base_fee_scalar": 0, "decimals": 6 } },
///       "operator_fee": { "constant": 0, "scalar": 0, "enabled": false }
///     }
///   }
/// }
pub fn parse_fee_handlers_config(extras: &Value) -> Result<FeeHandlersConfig, ConfigError> {
    let ev = extras.get("ev_reth").ok_or(ConfigError::Missing)?;
    let fh = ev.get("feeHandlers").ok_or(ConfigError::Missing)?;
    serde_json::from_value::<FeeHandlersConfig>(fh.clone())
        .map_err(|e| ConfigError::Invalid(e.to_string()))
}

/// Convenience helper if you want to build the config directly.
pub fn build_config(
    sequencer_fee_vault: Address,
    base_fee_vault: Address,
    l1_fee_vault: Address,
    operator_fee_vault: Option<Address>,
    l1_params: L1FeeParams,
    operator_fee: Option<OperatorFeeParams>,
) -> FeeHandlersConfig {
    FeeHandlersConfig {
        vaults: FeeVaults {
            sequencer_fee_vault,
            base_fee_vault,
            l1_fee_vault,
            operator_fee_vault,
        },
        l1_params,
        operator_fee: operator_fee.unwrap_or_default(),
    }
}
