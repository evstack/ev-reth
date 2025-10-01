// Mint precompile

use alloy::{sol, sol_types::SolInterface};
use alloy_evm::{
    precompiles::{Precompile, PrecompileInput},
    revm::precompile::{PrecompileError, PrecompileId, PrecompileResult},
};
use alloy_primitives::{address, Address, Bytes};
use revm::precompile::PrecompileOutput;
use std::sync::OnceLock;

sol! {
    interface INativeToken {
        function mint(address to, uint256 amount) external returns (bool ok);
        function burn(address from, uint256 amount) external returns (bool ok);
    }
}

pub const MINT_PRECOMPILE_ADDR: Address = address!("0x000000000000000000000000000000000000F100");

/// A custom precompile that mints the native token
#[derive(Clone, Debug, Default)]
pub struct MintPrecompile;

impl MintPrecompile {
    // Use a lazily-initialized static for the ID since `custom` is not const.
    pub fn id() -> &'static PrecompileId {
        static ID: OnceLock<PrecompileId> = OnceLock::new();
        ID.get_or_init(|| PrecompileId::custom("native_mint"))
    }

    pub fn new() -> Self {
        Self
    }
}

impl Precompile for MintPrecompile {
    fn precompile_id(&self) -> &PrecompileId {
        &Self::id()
    }

    /// Execute the precompile with the given input data, gas limit, and caller address.
    fn call(&self, input: PrecompileInput<'_>) -> PrecompileResult {
        // 1) Decode by ABI — this inspects the 4-byte selector and picks the right variant.
        let decoded = match INativeToken::INativeTokenCalls::abi_decode(input.data) {
            Ok(v) => v,
            Err(e) => return Err(PrecompileError::Other(e.to_string())),
        };

        // 2) Dispatch to the right handler.
        match decoded {
            INativeToken::INativeTokenCalls::mint(call) => {
                // call.to, call.amount
                // ... do state changes / balances / checks ...

                Ok(PrecompileOutput::new(input.gas, Bytes::new()))
            }
            INativeToken::INativeTokenCalls::burn(call) => {
                // call.from, call.amount
                Ok(PrecompileOutput::new(input.gas, Bytes::new()))
            }
        }
    }

    fn is_pure(&self) -> bool {
        true
    }
}
