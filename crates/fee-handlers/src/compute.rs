use crate::types::*;

/// Compute base fee (EIP-1559) for the block: base_fee_per_gas * gas_used.
#[inline]
pub fn compute_base_fee_wei(base_fee_per_gas: u64, gas_used: u64) -> u128 {
    (base_fee_per_gas as u128) * (gas_used as u128)
}

/// Compute L1 fee (V1) in wei, based on selected params.
///
/// Inputs:
/// - `mode/params`: from chainspec.
/// - `tx_bytes`: Bedrock → zero/nonzero + overhead; Ecotone/Fjord → compressed_size.
/// - `l1_base_fee`: the L1 base fee (wei), if available from your L1 block source; else 0.
/// - `l1_blob_base_fee`: the L1 blob base fee (wei), if applicable; else 0.
#[inline]
pub fn compute_l1_fee_wei(
    params: &L1FeeParams,
    tx_bytes: &TxBytesAcc,
    _l1_base_fee: u128,
    l1_blob_base_fee: u128,
) -> u128 {
    match params {
        L1FeeParams::V1 { v1 } => {
            // Celestia shares-based fee:
            // shares = ceil(total_size / share_size) + overhead_shares
            // fee = shares * (blob_price_scalar * l1_blob_base_fee) / 1e{decimals}
            let share_size = v1.share_size.max(1) as u64; // prevent div by zero
            let bytes = tx_bytes.total_size as u64;
            let shares = (bytes + share_size - 1) / share_size + (v1.overhead_shares as u64);
            let factor = (v1.blob_price_scalar as u128).saturating_mul(l1_blob_base_fee);
            let num = (shares as u128).saturating_mul(factor);
            let denom = 10u128.pow(v1.decimals);
            num / denom
        }
    }
}

/// Optional operator fee: constant + (scalar * gas_used / 1e6)
#[inline]
pub fn compute_operator_fee_wei(op: &OperatorFeeParams, gas_used: u64) -> u128 {
    if !op.enabled {
        return 0;
    }
    (op.constant as u128) + ((op.scalar as u128) * (gas_used as u128) / 1_000_000u128)
}

pub fn compute_totals(
    cfg: &crate::types::FeeHandlersConfig,
    base_fee_per_gas: u64,
    gas_used: u64,
    tx_bytes: &TxBytesAcc,
    l1_base_fee: u128,
    l1_blob_base_fee: u128,
) -> FeeTotals {
    let base_fee_wei = compute_base_fee_wei(base_fee_per_gas, gas_used);
    let l1_fee_wei = compute_l1_fee_wei(&cfg.l1_params, tx_bytes, l1_base_fee, l1_blob_base_fee);
    let operator_fee_wei = compute_operator_fee_wei(&cfg.operator_fee, gas_used);
    FeeTotals {
        base_fee_wei,
        l1_fee_wei,
        operator_fee_wei,
    }
}
