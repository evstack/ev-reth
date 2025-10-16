// Mint precompile

use alloy::{sol, sol_types::SolInterface};
use alloy_evm::{
    precompiles::{Precompile, PrecompileInput},
    revm::precompile::{PrecompileError, PrecompileId, PrecompileResult},
    EvmInternals, EvmInternalsError,
};
use alloy_primitives::{address, Address, Bytes, U256};
use revm::precompile::PrecompileOutput;
use std::sync::OnceLock;

sol! {
    interface INativeToken {
        function mint(address to, uint256 amount);
        function burn(address from, uint256 amount);
    }
}

pub const MINT_PRECOMPILE_ADDR: Address = address!("0x000000000000000000000000000000000000F100");

/// A custom precompile that mints the native token
#[derive(Clone, Debug, Default)]
pub struct MintPrecompile {
    admin: Address,
}

impl MintPrecompile {
    // Use a lazily-initialized static for the ID since `custom` is not const.
    pub fn id() -> &'static PrecompileId {
        static ID: OnceLock<PrecompileId> = OnceLock::new();
        ID.get_or_init(|| PrecompileId::custom("native_mint"))
    }

    pub fn new(admin: Address) -> Self {
        Self { admin }
    }

    fn is_authorized(&self, caller: Address) -> bool {
        caller == self.admin
    }

    fn map_internals_error(err: EvmInternalsError) -> PrecompileError {
        PrecompileError::Other(err.to_string())
    }

    fn ensure_account_created(
        internals: &mut EvmInternals<'_>,
        addr: Address,
    ) -> Result<(), PrecompileError> {
        let mut account = internals
            .load_account(addr)
            .map_err(Self::map_internals_error)?;

        if account.is_loaded_as_not_existing() {
            account.mark_created();
            internals.touch_account(addr);
        }

        Ok(())
    }

    fn add_balance(
        internals: &mut EvmInternals<'_>,
        addr: Address,
        amount: U256,
    ) -> Result<(), PrecompileError> {
        let mut account = internals
            .load_account(addr)
            .map_err(Self::map_internals_error)?;
        let new_balance = account
            .info
            .balance
            .checked_add(amount)
            .ok_or_else(|| PrecompileError::Other("balance overflow".to_string()))?;
        account.info.set_balance(new_balance);
        Ok(())
    }

    fn sub_balance(
        internals: &mut EvmInternals<'_>,
        addr: Address,
        amount: U256,
    ) -> Result<(), PrecompileError> {
        let mut account = internals
            .load_account(addr)
            .map_err(Self::map_internals_error)?;
        let new_balance = account
            .info
            .balance
            .checked_sub(amount)
            .ok_or_else(|| PrecompileError::Other("insufficient balance".to_string()))?;
        account.info.set_balance(new_balance);
        Ok(())
    }
}

impl Precompile for MintPrecompile {
    fn precompile_id(&self) -> &PrecompileId {
        &Self::id()
    }

    /// Execute the precompile with the given input data, gas limit, and caller address.
    fn call(&self, mut input: PrecompileInput<'_>) -> PrecompileResult {
        let caller: Address = input.caller;

        // Enforce access control.
        if !self.is_authorized(caller) {
            return Err(PrecompileError::Other("unauthorized caller".to_string()));
        }
        let gas_limit = input.gas;

        // 1) Decode by ABI — this inspects the 4-byte selector and picks the right variant.
        let decoded = match INativeToken::INativeTokenCalls::abi_decode(input.data) {
            Ok(v) => v,
            Err(e) => return Err(PrecompileError::Other(e.to_string())),
        };

        let internals = input.internals_mut();

        // 2) Dispatch to the right handler.
        match decoded {
            INativeToken::INativeTokenCalls::mint(call) => {
                let to = call.to;
                let amount = call.amount;

                internals.touch_account(to);
                Self::ensure_account_created(internals, to)?;
                Self::add_balance(internals, to, amount)?;

                Ok(PrecompileOutput::new(gas_limit, Bytes::new()))
            }
            INativeToken::INativeTokenCalls::burn(call) => {
                let from = call.from;
                let amount = call.amount;

                internals.touch_account(from);
                Self::ensure_account_created(internals, from)?;
                Self::sub_balance(internals, from, amount)?;

                Ok(PrecompileOutput::new(gas_limit, Bytes::new()))
            }
        }
    }

    fn is_pure(&self) -> bool {
        false
    }
}
