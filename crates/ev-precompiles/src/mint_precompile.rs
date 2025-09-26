
use alloy_primitives::{Address, U256};
use bytes::Bytes;
use revm::precompile::{Precompile, PrecompileError, PrecompileOutput};
use std::str;

/// The address of the native mint precompile.
pub const NATIVE_MINT_PRECOMPILE_ADDRESS: Address = Address::new([
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0xEF,
]);

/// The ABI for the native mint precompile.
///
/// The precompile accepts a 52-byte buffer, structured as follows:
/// - The first 20 bytes represent the recipient's address (`to`).
/// - The next 32 bytes represent the amount to mint (`amount`).
///
/// ```solidity
/// interface INativeMint {
///     function mint(address to, uint256 amount) external;
/// }
/// ```
pub const NATIVE_MINT_PRECOMPILE_ABI: &str = r#"[{"inputs":[{"internalType":"address","name":"to","type":"address"},{"internalType":"uint256","name":"amount","type":"uint256"}],"name":"mint","outputs":[],"stateMutability":"nonpayable","type":"function"}]"#;

/// The gas cost for the native mint precompile.
pub const NATIVE_MINT_PRECOMPILE_GAS_COST: u64 = 5_000;

/// A stateless precompile for minting the native token.
///
/// This precompile is responsible for parsing the input, validating the gas, and returning a
/// success or error. The actual state mutation (crediting the balance) is handled by the
/// `MintInspector`.
pub fn native_mint(input: &Bytes, gas_limit: u64) -> Result<PrecompileOutput, PrecompileError> {
    // Check if the gas limit is sufficient.
    if gas_limit < NATIVE_MINT_PRECOMPILE_GAS_COST {
        return Err(PrecompileError::OutOfGas);
    }

    // The input should be exactly 52 bytes long: 20 bytes for the address and 32 bytes for the
    // amount.
    if input.len() != 52 {
        return Err(PrecompileError::Other(
            "invalid input length".to_string().into(),
        ));
    }

    // Return success with the gas used. The actual minting is handled by the inspector.
    Ok(PrecompileOutput::new(
        NATIVE_MINT_PRECOMPILE_GAS_COST,
        Bytes::new(),
    ))
}
