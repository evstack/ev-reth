use alloy_primitives::Bytes;
use revm::precompile::{PrecompileError, PrecompileOutput, PrecompileResult};

/// The gas cost for the native mint/burn precompile.
pub const NATIVE_MINT_BURN_PRECOMPILE_GAS_COST: u64 = 5_000;

/// Function selectors (first 4 bytes of keccak256 hash)
/// mint(address,uint256) -> 0x40c10f19
const MINT_SELECTOR: [u8; 4] = [0x40, 0xc1, 0x0f, 0x19];
/// burn(uint256) -> 0x42966c68
const BURN_SELECTOR: [u8; 4] = [0x42, 0x96, 0x6c, 0x68];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Operation {
    Mint,
    Burn,
}

/// A stateless precompile for minting and burning the native token.
///
/// Supports two operations via function selectors:
///
/// **mint(address,uint256)** - Mints tokens to a specified address
/// - Selector: 0x40c10f19
/// - Input: 4 bytes selector + 20 bytes address + 32 bytes amount (56 bytes total)
/// - Effect: Increases balance of target address by amount
///
/// **burn(uint256)** - Burns tokens from the caller's balance
/// - Selector: 0x42966c68
/// - Input: 4 bytes selector + 32 bytes amount (36 bytes total)
/// - Effect: Decreases caller's balance by amount
pub fn native_mint_burn(input: &[u8], gas_limit: u64) -> PrecompileResult {
    if gas_limit < NATIVE_MINT_BURN_PRECOMPILE_GAS_COST {
        return Err(PrecompileError::OutOfGas);
    }

    if input.len() < 4 {
        return Err(PrecompileError::Other("input too short".to_string()));
    }

    let selector = &input[0..4];

    match selector {
        s if s == MINT_SELECTOR => {
            // mint(address,uint256) - expects 56 bytes total
            if input.len() != 56 {
                return Err(PrecompileError::Other(
                    "invalid mint input length".to_string(),
                ));
            }
        }
        s if s == BURN_SELECTOR => {
            // burn(uint256) - expects 36 bytes total
            if input.len() != 36 {
                return Err(PrecompileError::Other(
                    "invalid burn input length".to_string(),
                ));
            }
        }
        _ => {
            return Err(PrecompileError::Other(
                "unknown function selector".to_string(),
            ));
        }
    }

    Ok(PrecompileOutput::new(
        NATIVE_MINT_BURN_PRECOMPILE_GAS_COST,
        Bytes::new(),
    ))
}

/// Parse the operation type from input
pub fn parse_operation(input: &[u8]) -> Result<Operation, PrecompileError> {
    if input.len() < 4 {
        return Err(PrecompileError::Other("input too short".to_string()));
    }

    let selector = &input[0..4];
    match selector {
        s if s == MINT_SELECTOR => Ok(Operation::Mint),
        s if s == BURN_SELECTOR => Ok(Operation::Burn),
        _ => Err(PrecompileError::Other(
            "unknown function selector".to_string(),
        )),
    }
}
