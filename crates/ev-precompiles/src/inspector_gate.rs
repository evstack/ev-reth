use crate::config::MintConfig;
use alloy_primitives::{Address, U256};
use revm::{
    context_interface::{ContextTr, JournalTr},
    inspector::Inspector,
    interpreter::{CallInputs, CallOutcome, Gas, InstructionResult, InterpreterResult},
};

#[derive(Clone, Debug)]
pub struct MintInspector {
    precompile: Address,
    allow: std::collections::HashSet<Address>,
    per_call_cap: U256,
    per_block_cap: Option<U256>,
    minted_this_block: U256,
}

impl MintInspector {
    pub fn new(config: MintConfig) -> Self {
        Self {
            precompile: config.precompile_address,
            allow: config.allow_list,
            per_call_cap: config.per_call_cap,
            per_block_cap: config.per_block_cap,
            minted_this_block: U256::ZERO,
        }
    }

    const MINT_CALLDATA_LEN: usize = 20 + 32;

    fn revert_outcome(message: &str, inputs: &CallInputs) -> CallOutcome {
        CallOutcome::new(
            Self::revert_result(message),
            inputs.return_memory_offset.clone(),
        )
    }

    fn revert_result(message: &str) -> InterpreterResult {
        InterpreterResult {
            result: InstructionResult::Revert,
            output: message.as_bytes().to_vec().into(),
            gas: Gas::new(0),
        }
    }
}

impl<CTX> Inspector<CTX> for MintInspector
where
    CTX: ContextTr,
{
    fn call(&mut self, context: &mut CTX, inputs: &mut CallInputs) -> Option<CallOutcome> {
        if inputs.target_address != self.precompile {
            return None;
        }

        if !self.allow.contains(&inputs.caller) {
            return Some(Self::revert_outcome("unauthorized", inputs));
        }

        let calldata = inputs.input.bytes(context);
        if calldata.len() != Self::MINT_CALLDATA_LEN {
            return Some(Self::revert_outcome("invalid input length", inputs));
        }

        let amount = U256::from_be_slice(&calldata[20..Self::MINT_CALLDATA_LEN]);
        if amount > self.per_call_cap {
            return Some(Self::revert_outcome("over per-call cap", inputs));
        }

        if let Some(cap) = self.per_block_cap {
            if self.minted_this_block + amount > cap {
                return Some(Self::revert_outcome("over per-block cap", inputs));
            }
        }

        None
    }

    fn call_end(&mut self, context: &mut CTX, inputs: &CallInputs, outcome: &mut CallOutcome) {
        if inputs.target_address != self.precompile || !outcome.result.is_ok() {
            return;
        }

        let calldata = inputs.input.bytes(context);
        if calldata.len() != Self::MINT_CALLDATA_LEN {
            outcome.result = Self::revert_result("invalid input length");
            return;
        }

        let to = Address::from_slice(&calldata[..20]);
        let amount = U256::from_be_slice(&calldata[20..Self::MINT_CALLDATA_LEN]);

        match context.journal_mut().load_account(to) {
            Ok(mut account_load) => {
                account_load.info.balance += amount;
            }
            Err(_) => {
                outcome.result = Self::revert_result("account load failed");
                return;
            }
        }

        self.minted_this_block += amount;
    }
}
