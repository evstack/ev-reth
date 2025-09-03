use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

// V1 fee parameters: Celestia shares-based model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct V1Params {
    /// Share size in bytes (Celestia uses 512-byte shares).
    #[serde(default = "default_share_size")]
    pub share_size: u32,
    /// Fixed overhead shares per block (framing/metadata).
    #[serde(default)]
    pub overhead_shares: u32,
    /// Scalar applied to blob base fee per share.
    #[serde(default = "default_blob_price_scalar")]
    pub blob_price_scalar: u64,
    /// Decimal scaling for the scalar.
    #[serde(default = "default_decimals")]
    pub decimals: u32,
}

// Keep an enum for forward-compatibility, with a single V1 variant for now.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum L1FeeParams {
    #[serde(rename = "V1")]
    V1 { v1: V1Params },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OperatorFeeParams {
    /// Additive constant (in wei).
    pub constant: u64,
    /// Scalar applied to gas_used / 1e6 (in wei).
    pub scalar: u64,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeVaults {
    pub sequencer_fee_vault: Address,
    pub base_fee_vault: Address,
    pub l1_fee_vault: Address,
    #[serde(default)]
    pub operator_fee_vault: Option<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeHandlersConfig {
    pub vaults: FeeVaults,
    pub l1_params: L1FeeParams,
    #[serde(default)]
    pub operator_fee: OperatorFeeParams,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TxBytesAcc {
    /// Total DA payload size proxy for the block (bytes).
    /// In Celestia V1 we approximate by summing the RLP-encoded
    /// transaction bytes included in the block.
    pub total_size: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FeeTotals {
    pub base_fee_wei: u128,
    pub l1_fee_wei: u128,
    pub operator_fee_wei: u128,
}

const fn default_decimals() -> u32 { 6 }
const fn default_share_size() -> u32 { 512 }
const fn default_blob_price_scalar() -> u64 { 1_000_000 }
