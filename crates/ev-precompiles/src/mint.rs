// Mint precompile

use alloy::{sol, sol_types::SolInterface};
use alloy_evm::{
    precompiles::{Precompile, PrecompileInput},
    revm::precompile::{PrecompileError, PrecompileId, PrecompileResult},
    EvmInternals, EvmInternalsError,
};
use alloy_primitives::{address, Address, Bytes, U256};
use revm::bytecode::Bytecode;
use revm::precompile::PrecompileOutput;
use std::sync::OnceLock;

sol! {
    interface INativeToken {
        function mint(address to, uint256 amount);
        function burn(address from, uint256 amount);
        function addToAllowList(address account);
        function removeFromAllowList(address account);
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

    fn bytecode() -> &'static Bytecode {
        static BYTECODE: OnceLock<Bytecode> = OnceLock::new();
        BYTECODE.get_or_init(|| Bytecode::new_raw(Bytes::from_static(&[0xFE])))
    }

    pub fn new(admin: Address) -> Self {
        Self { admin }
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
            if addr == MINT_PRECOMPILE_ADDR {
                // Ensure the mint precompile account is treated as non-empty so state pruning
                // does not wipe out its storage between blocks.
                account.info.nonce = 1;
                internals.set_code(addr, Self::bytecode().clone());
            }
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

    fn ensure_admin(&self, caller: Address) -> Result<(), PrecompileError> {
        if caller == self.admin {
            Ok(())
        } else {
            Err(PrecompileError::Other("unauthorized caller".to_string()))
        }
    }

    fn ensure_authorized(
        &self,
        internals: &mut EvmInternals<'_>,
        caller: Address,
    ) -> Result<(), PrecompileError> {
        if caller == self.admin {
            tracing::debug!(target: "mint_precompile", ?caller, "authorization granted: admin");
            return Ok(());
        }

        let allowlisted = Self::is_allowlisted(internals, caller)?;
        if allowlisted {
            tracing::debug!(target: "mint_precompile", ?caller, "authorization granted: allowlist");
            Ok(())
        } else {
            tracing::warn!(target: "mint_precompile", ?caller, "authorization denied: not admin and not allowlisted");
            Err(PrecompileError::Other("unauthorized caller".to_string()))
        }
    }

    fn is_allowlisted(
        internals: &mut EvmInternals<'_>,
        addr: Address,
    ) -> Result<bool, PrecompileError> {
        Self::ensure_account_created(internals, MINT_PRECOMPILE_ADDR)?;
        let key = Self::allowlist_key(addr);
        let value = internals
            .sload(MINT_PRECOMPILE_ADDR, key)
            .map_err(Self::map_internals_error)?;
        let raw_value = *value;
        let allowlisted = !raw_value.is_zero();
        tracing::debug!(
            target: "mint_precompile",
            ?addr,
            slot = %key,
            value = %raw_value,
            allowlisted,
            "allowlist lookup"
        );
        Ok(allowlisted)
    }

    fn set_allowlisted(
        internals: &mut EvmInternals<'_>,
        addr: Address,
        allowed: bool,
    ) -> Result<(), PrecompileError> {
        Self::ensure_account_created(internals, MINT_PRECOMPILE_ADDR)?;
        let value = if allowed { U256::from(1) } else { U256::ZERO };
        internals
            .sstore(MINT_PRECOMPILE_ADDR, Self::allowlist_key(addr), value)
            .map_err(Self::map_internals_error)?;
        internals.touch_account(MINT_PRECOMPILE_ADDR);
        Ok(())
    }

    fn allowlist_key(addr: Address) -> U256 {
        U256::from_be_bytes(addr.into_word().into())
    }
}

impl Precompile for MintPrecompile {
    fn precompile_id(&self) -> &PrecompileId {
        Self::id()
    }

    /// Execute the precompile with the given input data, gas limit, and caller address.
    fn call(&self, mut input: PrecompileInput<'_>) -> PrecompileResult {
        let caller: Address = input.caller;
        let gas_limit = input.gas;
        let data_len = input.data.len();

        tracing::info!(
            target: "mint_precompile",
            ?caller,
            gas = gas_limit,
            calldata_len = data_len,
            "mint precompile call invoked"
        );

        // 1) Decode by ABI — this inspects the 4-byte selector and picks the right variant.
        let decoded = match INativeToken::INativeTokenCalls::abi_decode(input.data) {
            Ok(v) => v,
            Err(e) => return Err(PrecompileError::Other(e.to_string())),
        };
        let internals = input.internals_mut();

        // 2) Dispatch to the right handler.
        match decoded {
            INativeToken::INativeTokenCalls::mint(call) => {
                self.ensure_authorized(internals, caller)?;
                let to = call.to;
                let amount = call.amount;

                Self::ensure_account_created(internals, to)?;
                Self::add_balance(internals, to, amount)?;
                internals.touch_account(to);

                Ok(PrecompileOutput::new(0, Bytes::new()))
            }
            INativeToken::INativeTokenCalls::burn(call) => {
                self.ensure_authorized(internals, caller)?;
                let from = call.from;
                let amount = call.amount;

                Self::ensure_account_created(internals, from)?;
                Self::sub_balance(internals, from, amount)?;
                internals.touch_account(from);

                Ok(PrecompileOutput::new(0, Bytes::new()))
            }
            INativeToken::INativeTokenCalls::addToAllowList(call) => {
                self.ensure_admin(caller)?;
                Self::set_allowlisted(internals, call.account, true)?;
                Ok(PrecompileOutput::new(0, Bytes::new()))
            }
            INativeToken::INativeTokenCalls::removeFromAllowList(call) => {
                self.ensure_admin(caller)?;
                Self::set_allowlisted(internals, call.account, false)?;
                Ok(PrecompileOutput::new(0, Bytes::new()))
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

        let output = run_call(&mut journal, &block_env, &precompile, admin, &calldata)
            .expect("mint call should succeed");
        assert_eq!(output.gas_used, 0, "mint precompile should not consume gas");
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
        let mint_output = run_call(&mut journal, &block_env, &precompile, admin, &mint_calldata)
            .expect("mint call should succeed");
        assert_eq!(
            mint_output.gas_used, 0,
            "mint precompile should not consume gas"
        );
        let burn_calldata = INativeToken::burnCall {
            from: holder,
            amount: burn_amount,
        }
        .abi_encode();

        let burn_output = run_call(&mut journal, &block_env, &precompile, admin, &burn_calldata)
            .expect("burn call should succeed");
        assert_eq!(
            burn_output.gas_used, 0,
            "burn precompile should not consume gas"
        );
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
            !journal.inner.state.contains_key(&recipient),
            "unauthorized call must not create new accounts"
        );
    }

    #[test]
    fn allowlisted_caller_can_mint() {
        let admin = address!("0x00000000000000000000000000000000000000a5");
        let allowlisted = address!("0x00000000000000000000000000000000000000b5");
        let recipient = address!("0x00000000000000000000000000000000000000c5");
        let amount = U256::from(77u64);
        let precompile = MintPrecompile::new(admin);

        let (mut journal, block_env) = setup_context();

        let add_calldata = INativeToken::addToAllowListCall {
            account: allowlisted,
        }
        .abi_encode();
        let add_output = run_call(&mut journal, &block_env, &precompile, admin, &add_calldata)
            .expect("admin should be able to add to allowlist");
        assert_eq!(
            add_output.gas_used, 0,
            "allowlist add should not consume gas"
        );

        let mint_calldata = INativeToken::mintCall {
            to: recipient,
            amount,
        }
        .abi_encode();
        let mint_output = run_call(
            &mut journal,
            &block_env,
            &precompile,
            allowlisted,
            &mint_calldata,
        )
        .expect("allowlisted caller should be able to mint");
        assert_eq!(
            mint_output.gas_used, 0,
            "mint for allowlisted caller should not consume gas"
        );

        let balance = account_balance(&journal, recipient).expect("recipient exists");
        assert_eq!(balance, amount, "recipient receives minted amount");
    }

    #[test]
    fn removing_allowlisted_address_revokes_access() {
        let admin = address!("0x00000000000000000000000000000000000000a6");
        let allowlisted = address!("0x00000000000000000000000000000000000000b6");
        let recipient = address!("0x00000000000000000000000000000000000000c6");
        let amount = U256::from(15u64);
        let precompile = MintPrecompile::new(admin);

        let (mut journal, block_env) = setup_context();

        let add_calldata = INativeToken::addToAllowListCall {
            account: allowlisted,
        }
        .abi_encode();
        let add_output = run_call(&mut journal, &block_env, &precompile, admin, &add_calldata)
            .expect("admin should be able to add allowlist entry");
        assert_eq!(
            add_output.gas_used, 0,
            "allowlist add should not consume gas"
        );

        let remove_calldata = INativeToken::removeFromAllowListCall {
            account: allowlisted,
        }
        .abi_encode();
        let remove_output = run_call(
            &mut journal,
            &block_env,
            &precompile,
            admin,
            &remove_calldata,
        )
        .expect("admin should be able to remove allowlist entry");
        assert_eq!(
            remove_output.gas_used, 0,
            "allowlist removal should not consume gas"
        );

        let mint_calldata = INativeToken::mintCall {
            to: recipient,
            amount,
        }
        .abi_encode();
        let result = run_call(
            &mut journal,
            &block_env,
            &precompile,
            allowlisted,
            &mint_calldata,
        );

        match result {
            Err(PrecompileError::Other(msg)) => assert_eq!(
                msg, "unauthorized caller",
                "removed address should no longer be authorized"
            ),
            other => panic!("expected unauthorized error, got {other:?}"),
        }

        assert!(
            !journal.inner.state.contains_key(&recipient),
            "revoked caller must not mint"
        );
    }

    #[test]
    fn allowlisted_caller_can_burn() {
        let admin = address!("0x00000000000000000000000000000000000000a8");
        let allowlisted = address!("0x00000000000000000000000000000000000000b8");
        let holder = address!("0x00000000000000000000000000000000000000c8");
        let mint_amount = U256::from(90u64);
        let burn_amount = U256::from(40u64);
        let precompile = MintPrecompile::new(admin);

        let (mut journal, block_env) = setup_context();

        // Add operator to allowlist
        let add_calldata = INativeToken::addToAllowListCall {
            account: allowlisted,
        }
        .abi_encode();
        let add_output = run_call(&mut journal, &block_env, &precompile, admin, &add_calldata)
            .expect("admin should add allowlist entry");
        assert_eq!(
            add_output.gas_used, 0,
            "allowlist add should not consume gas"
        );

        // Mint tokens as allowlisted operator
        let mint_calldata = INativeToken::mintCall {
            to: holder,
            amount: mint_amount,
        }
        .abi_encode();
        let mint_output = run_call(
            &mut journal,
            &block_env,
            &precompile,
            allowlisted,
            &mint_calldata,
        )
        .expect("allowlisted operator should mint");
        assert_eq!(
            mint_output.gas_used, 0,
            "allowlisted mint should not consume gas"
        );

        // Burn subset as allowlisted operator
        let burn_calldata = INativeToken::burnCall {
            from: holder,
            amount: burn_amount,
        }
        .abi_encode();
        let burn_output = run_call(
            &mut journal,
            &block_env,
            &precompile,
            allowlisted,
            &burn_calldata,
        )
        .expect("allowlisted operator should burn");
        assert_eq!(
            burn_output.gas_used, 0,
            "allowlisted burn should not consume gas"
        );

        let balance = account_balance(&journal, holder).expect("holder account exists");
        assert_eq!(
            balance,
            mint_amount - burn_amount,
            "burn should reduce balance for allowlisted operator"
        );
    }

    #[test]
    fn non_admin_cannot_modify_allowlist() {
        let admin = address!("0x00000000000000000000000000000000000000a7");
        let unauthorized = address!("0x00000000000000000000000000000000000000f7");
        let target = address!("0x00000000000000000000000000000000000000b7");
        let precompile = MintPrecompile::new(admin);

        let (mut journal, block_env) = setup_context();
        let add_calldata = INativeToken::addToAllowListCall { account: target }.abi_encode();

        let result = run_call(
            &mut journal,
            &block_env,
            &precompile,
            unauthorized,
            &add_calldata,
        );

        match result {
            Err(PrecompileError::Other(msg)) => assert_eq!(
                msg, "unauthorized caller",
                "non-admin must not modify allowlist"
            ),
            other => panic!("expected unauthorized error, got {other:?}"),
        }
    }
}
