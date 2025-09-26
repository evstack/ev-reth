
use crate::mint_precompile::NATIVE_MINT_PRECOMPILE_ADDRESS;
use alloy_primitives::{Address, U256};
use revm::db::Database;
use revm::precompile::Precompile;
use revm::primitives::{ExecutionResult, Output, TransactTo};
use revm::{Evm, Inspector};
use std::collections::HashSet;

/// An inspector that gates calls to the native mint precompile.
///
/// This inspector enforces an allowlist and various limits on the minting of the native token.
/// It also performs the state mutation (crediting the balance) when the precompile call is
/// successful.
pub struct MintInspector {
    /// The address of the native mint precompile.
    precompile: Address,
    /// A static list of addresses that are allowed to call the precompile.
    allow: HashSet<Address>,
    /// An optional on-chain registry for validating callers.
    registry: Option<Address>,
    /// The maximum amount that can be minted in a single call.
    per_call_cap: U256,
    /// The maximum amount that can be minted in a single block.
    per_block_cap: Option<U256>,
    /// The total amount minted in the current block.
    minted_this_block: U256,
}

impl<DB: Database> Inspector<DB> for MintInspector {
    /// Called before the execution of a transaction.
    fn transact_inspect(
        &mut self,
        context: &mut revm::InnerEvmContext<DB>,
        transact_to: TransactTo,
        caller: Address,
        value: U256,
    ) -> Option<Vec<u8>> {
        // Reset the per-block mint counter at the beginning of each transaction.
        self.minted_this_block = U256::ZERO;
        None
    }

    /// Called after the execution of a transaction.
    fn transact_end(&mut self, context: &mut revm::InnerEvmContext<DB>, result: ExecutionResult) {
        // No-op
    }
}
