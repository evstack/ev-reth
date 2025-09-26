use crate::mint_precompile::NATIVE_MINT_PRECOMPILE_ADDRESS;
use alloy_primitives::{Address, U256};
use bytes::Bytes;
use revm::db::Database;
use revm::primitives::{
    AccountInfo, CallInputs, CallOutcome, ExecutionResult, Output, TransactTo, B160,
};
use revm::{DatabaseCommit, Evm, Inspector, JournaledState};
use std::collections::HashSet;

/// An inspector that gates calls to the native mint precompile.
///
/// This inspector enforces an allowlist and various limits on the minting of the native token.
/// It also performs the state mutation (crediting the balance) when the precompile call is
/// successful.
#[derive(Clone, Debug)]
pub struct MintInspector {
    /// The address of the native mint precompile.
    precompile: Address,
    /// A static list of addresses that are allowed to call the precompile.
    allow: HashSet<Address>,
    /// The maximum amount that can be minted in a single call.
    per_call_cap: U256,
    /// The maximum amount that can be minted in a single block.
    per_block_cap: Option<U256>,
    /// The total amount minted in the current block.
    minted_this_block: U256,
}

impl<DB: Database> Inspector<DB> for MintInspector {
    fn call(
        &mut self,
        context: &mut revm::InnerEvmContext<DB>,
        inputs: &mut CallInputs,
    ) -> Option<CallOutcome> {
        // Intercept calls to the native mint precompile.
        if inputs.contract != self.precompile {
            return None;
        }

        // Check if the caller is authorized.
        if !self.allow.contains(&inputs.caller) {
            // Return a revert outcome if the caller is not authorized.
            let outcome = CallOutcome::new().with_revert(Bytes::from_static(b"unauthorized"));
            return Some(outcome);
        }

        // Decode the amount from the input.
        let amount = U256::from_be_slice(&inputs.input[20..52]);

        // Check if the amount exceeds the per-call cap.
        if amount > self.per_call_cap {
            let outcome = CallOutcome::new().with_revert(Bytes::from_static(b"over per-call cap"));
            return Some(outcome);
        }

        // Check if the amount exceeds the per-block cap.
        if let Some(per_block_cap) = self.per_block_cap {
            if self.minted_this_block + amount > per_block_cap {
                let outcome =
                    CallOutcome::new().with_revert(Bytes::from_static(b"over per-block cap"));
                return Some(outcome);
            }
        }

        None
    }

    fn call_end(
        &mut self,
        context: &mut revm::InnerEvmContext<DB>,
        inputs: &CallInputs,
        outcome: CallOutcome,
    ) -> CallOutcome {
        // Only handle successful calls to the native mint precompile.
        if inputs.contract != self.precompile || outcome.is_revert() {
            return outcome;
        }

        // Decode the recipient and amount from the input.
        let to = Address::from_slice(&inputs.input[0..20]);
        let amount = U256::from_be_slice(&inputs.input[20..52]);

        // Credit the recipient's balance.
        // This is the core state mutation logic.
        let (account, _) = context.journaled_state.load_account(to, context.db).unwrap();
        account.info.balance += amount;
        self.minted_this_block += amount;

        // Mark the account as touched so the state change is persisted.
        context.journaled_state.touch(&to);

        outcome
    }
}