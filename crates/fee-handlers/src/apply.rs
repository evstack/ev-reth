use crate::types::{FeeHandlersConfig, FeeTotals};
use alloy_primitives::Address;

/// Produce the balance credits to apply for this block.
/// The node/builder should apply these as post-block state deltas.
pub fn credit_plan(cfg: &FeeHandlersConfig, totals: &FeeTotals) -> Vec<(Address, u128)> {
    let mut v = Vec::with_capacity(3);
    if totals.base_fee_wei != 0 {
        v.push((cfg.vaults.base_fee_vault, totals.base_fee_wei));
    }
    if totals.l1_fee_wei != 0 {
        v.push((cfg.vaults.l1_fee_vault, totals.l1_fee_wei));
    }
    if totals.operator_fee_wei != 0 {
        if let Some(addr) = cfg.vaults.operator_fee_vault {
            v.push((addr, totals.operator_fee_wei));
        }
    }
    v
}
