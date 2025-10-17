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
        Self::id()
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::sol_types::SolCall;
    use alloy_primitives::address;
    use revm::{
        context::{
            journal::{Journal, JournalInner},
            BlockEnv,
        },
        database::{CacheDB, EmptyDB},
        primitives::hardfork::SpecId,
    };

    type TestJournal = Journal<CacheDB<EmptyDB>>;

    const GAS_LIMIT: u64 = 1_000_000;

    fn setup_context() -> (TestJournal, BlockEnv) {
        let mut journal = Journal::new_with_inner(CacheDB::default(), JournalInner::new());
        journal.inner.set_spec_id(SpecId::PRAGUE);
        let block_env = BlockEnv::default();
        (journal, block_env)
    }

    fn run_call<'a>(
        journal: &'a mut TestJournal,
        block_env: &'a BlockEnv,
        precompile: &MintPrecompile,
        caller: Address,
        data: &'a [u8],
    ) -> PrecompileResult {
        let input = PrecompileInput {
            data,
            gas: GAS_LIMIT,
            caller,
            value: U256::ZERO,
            target_address: MINT_PRECOMPILE_ADDR,
            bytecode_address: MINT_PRECOMPILE_ADDR,
            internals: EvmInternals::new(journal, block_env),
        };

        precompile.call(input)
    }

    fn account_balance(journal: &TestJournal, address: Address) -> Option<U256> {
        journal
            .inner
            .state
            .get(&address)
            .map(|account| account.info.balance)
    }

    #[test]
    fn mint_increases_balance() {
        let admin = address!("0x00000000000000000000000000000000000000a1");
        let recipient = address!("0x00000000000000000000000000000000000000b1");
        let amount = U256::from(42u64);
        let precompile = MintPrecompile::new(admin);

        let (mut journal, block_env) = setup_context();
        let calldata = INativeToken::mintCall {
            to: recipient,
            amount,
        }
        .abi_encode();

        let result = run_call(&mut journal, &block_env, &precompile, admin, &calldata);

        assert!(result.is_ok(), "mint call should succeed");
        let balance = account_balance(&journal, recipient).expect("recipient account exists");
        assert_eq!(
            balance, amount,
            "recipient balance must increase by minted amount"
        );

        let account = journal.inner.state.get(&recipient).unwrap();
        assert!(
            account.is_touched(),
            "recipient account should be marked touched"
        );
        assert!(
            account.is_created(),
            "recipient account should be marked created"
        );
    }

    #[test]
    fn burn_decreases_balance() {
        let admin = address!("0x00000000000000000000000000000000000000a2");
        let holder = address!("0x00000000000000000000000000000000000000b2");
        let mint_amount = U256::from(100u64);
        let burn_amount = U256::from(60u64);
        let precompile = MintPrecompile::new(admin);

        let (mut journal, block_env) = setup_context();
        let mint_calldata = INativeToken::mintCall {
            to: holder,
            amount: mint_amount,
        }
        .abi_encode();
        run_call(&mut journal, &block_env, &precompile, admin, &mint_calldata)
            .expect("mint call should succeed");
        let burn_calldata = INativeToken::burnCall {
            from: holder,
            amount: burn_amount,
        }
        .abi_encode();

        let result = run_call(&mut journal, &block_env, &precompile, admin, &burn_calldata);

        assert!(result.is_ok(), "burn call should succeed");
        let balance = account_balance(&journal, holder).expect("holder account exists");
        assert_eq!(
            balance,
            mint_amount - burn_amount,
            "holder balance must decrease"
        );
    }

    #[test]
    fn burn_underflow_is_rejected() {
        let admin = address!("0x00000000000000000000000000000000000000a3");
        let holder = address!("0x00000000000000000000000000000000000000b3");
        let initial_amount = U256::from(25u64);
        let burn_amount = U256::from(50u64);
        let precompile = MintPrecompile::new(admin);

        let (mut journal, block_env) = setup_context();
        let mint_calldata = INativeToken::mintCall {
            to: holder,
            amount: initial_amount,
        }
        .abi_encode();
        run_call(&mut journal, &block_env, &precompile, admin, &mint_calldata)
            .expect("mint call should succeed");
        let burn_calldata = INativeToken::burnCall {
            from: holder,
            amount: burn_amount,
        }
        .abi_encode();

        let result = run_call(&mut journal, &block_env, &precompile, admin, &burn_calldata);

        match result {
            Err(PrecompileError::Other(msg)) => {
                assert_eq!(
                    msg, "insufficient balance",
                    "expected insufficient balance error"
                )
            }
            other => panic!("expected underflow error, got {other:?}"),
        }

        let balance = account_balance(&journal, holder).expect("holder account exists");
        assert_eq!(
            balance, initial_amount,
            "balance should remain unchanged on underflow"
        );
    }

    #[test]
    fn unauthorized_caller_is_denied() {
        let admin = address!("0x00000000000000000000000000000000000000a4");
        let caller = address!("0x00000000000000000000000000000000000000ff");
        let recipient = address!("0x00000000000000000000000000000000000000cc");
        let amount = U256::from(10u64);
        let precompile = MintPrecompile::new(admin);

        let (mut journal, block_env) = setup_context();
        let calldata = INativeToken::mintCall {
            to: recipient,
            amount,
        }
        .abi_encode();

        let result = run_call(&mut journal, &block_env, &precompile, caller, &calldata);

        match result {
            Err(PrecompileError::Other(msg)) => {
                assert_eq!(
                    msg, "unauthorized caller",
                    "expected unauthorized caller error"
                )
            }
            other => panic!("expected unauthorized error, got {other:?}"),
        }

        assert!(
            !journal.inner.state.get(&recipient),
            "unauthorized call must not create new accounts"
        );
    }
}
