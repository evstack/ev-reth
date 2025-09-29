use alloy_primitives::Bytes;
use revm::precompile::{PrecompileError, PrecompileOutput, PrecompileResult};

/// The gas cost for the native mint precompile.
pub const NATIVE_MINT_PRECOMPILE_GAS_COST: u64 = 5_000;

/// A stateless precompile for minting the native token.
pub fn native_mint(input: &[u8], gas_limit: u64) -> PrecompileResult {
    if gas_limit < NATIVE_MINT_PRECOMPILE_GAS_COST {
        return Err(PrecompileError::OutOfGas);
    }

    if input.len() != 52 {
        return Err(PrecompileError::Other("invalid input length".to_string()));
    }

    Ok(PrecompileOutput::new(
        NATIVE_MINT_PRECOMPILE_GAS_COST,
        Bytes::new(),
    ))
}
