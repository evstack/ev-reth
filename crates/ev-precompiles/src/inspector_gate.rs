use crate::{
    config::MintConfig,
    mint_precompile::{parse_operation, Operation},
};
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
    burned_this_block: U256,
}

impl MintInspector {
    pub fn new(config: MintConfig) -> Self {
        Self {
            precompile: config.precompile_address,
            allow: config.allow_list,
            per_call_cap: config.per_call_cap,
            per_block_cap: config.per_block_cap,
            minted_this_block: U256::ZERO,
            burned_this_block: U256::ZERO,
        }
    }

    const MINT_CALLDATA_LEN: usize = 4 + 20 + 32; // selector + address + amount
    const BURN_CALLDATA_LEN: usize = 4 + 32; // selector + amount

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

        // Parse operation type
        let operation = match parse_operation(&calldata) {
            Ok(op) => op,
            Err(_) => return Some(Self::revert_outcome("invalid operation", inputs)),
        };

        // Validate length based on operation
        match operation {
            Operation::Mint => {
                if calldata.len() != Self::MINT_CALLDATA_LEN {
                    return Some(Self::revert_outcome("invalid mint input length", inputs));
                }
                let amount = U256::from_be_slice(&calldata[24..Self::MINT_CALLDATA_LEN]);
                if amount > self.per_call_cap {
                    return Some(Self::revert_outcome("over per-call cap", inputs));
                }
                if let Some(cap) = self.per_block_cap {
                    if self.minted_this_block + amount > cap {
                        return Some(Self::revert_outcome("over per-block cap", inputs));
                    }
                }
            }
            Operation::Burn => {
                if calldata.len() != Self::BURN_CALLDATA_LEN {
                    return Some(Self::revert_outcome("invalid burn input length", inputs));
                }
                let amount = U256::from_be_slice(&calldata[4..Self::BURN_CALLDATA_LEN]);
                if amount > self.per_call_cap {
                    return Some(Self::revert_outcome("over per-call cap", inputs));
                }
                if let Some(cap) = self.per_block_cap {
                    if self.burned_this_block + amount > cap {
                        return Some(Self::revert_outcome("over per-block cap", inputs));
                    }
                }
            }
        }

        None
    }

    fn call_end(&mut self, context: &mut CTX, inputs: &CallInputs, outcome: &mut CallOutcome) {
        if inputs.target_address != self.precompile || !outcome.result.is_ok() {
            return;
        }

        let calldata = inputs.input.bytes(context);

        // Parse operation type
        let operation = match parse_operation(&calldata) {
            Ok(op) => op,
            Err(_) => {
                outcome.result = Self::revert_result("invalid operation");
                return;
            }
        };

        match operation {
            Operation::Mint => {
                if calldata.len() != Self::MINT_CALLDATA_LEN {
                    outcome.result = Self::revert_result("invalid mint input length");
                    return;
                }

                let to = Address::from_slice(&calldata[4..24]);
                let amount = U256::from_be_slice(&calldata[24..Self::MINT_CALLDATA_LEN]);

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
            Operation::Burn => {
                if calldata.len() != Self::BURN_CALLDATA_LEN {
                    outcome.result = Self::revert_result("invalid burn input length");
                    return;
                }

                let amount = U256::from_be_slice(&calldata[4..Self::BURN_CALLDATA_LEN]);

                // Burn from caller's balance
                match context.journal_mut().load_account(inputs.caller) {
                    Ok(mut account_load) => {
                        if account_load.info.balance < amount {
                            outcome.result = Self::revert_result("insufficient balance");
                            return;
                        }
                        account_load.info.balance -= amount;
                    }
                    Err(_) => {
                        outcome.result = Self::revert_result("account load failed");
                        return;
                    }
                }

                self.burned_this_block += amount;
            }
        }
    }
}
