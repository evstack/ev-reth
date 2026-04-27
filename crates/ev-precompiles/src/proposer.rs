// Proposer control precompile

use alloy::{
    sol,
    sol_types::{SolInterface, SolValue},
};
use alloy_evm::{
    precompiles::{Precompile, PrecompileInput},
    revm::precompile::{PrecompileError, PrecompileId, PrecompileResult},
    EvmInternals, EvmInternalsError,
};
use alloy_primitives::{address, Address, Bytes, U256};
use revm::{
    bytecode::Bytecode,
    precompile::{PrecompileHalt, PrecompileOutput},
};
use std::sync::OnceLock;

sol! {
    interface IProposerControl {
        function nextProposer() external view returns (address);
        function setNextProposer(address proposer) external;
        function admin() external view returns (address);
    }
}

pub const PROPOSER_CONTROL_PRECOMPILE_ADDR: Address =
    address!("0x000000000000000000000000000000000000F101");

const NEXT_PROPOSER_SLOT: U256 = U256::ZERO;

/// A custom precompile that stores the next ev-node proposer in execution state.
#[derive(Clone, Debug, Default)]
pub struct ProposerControlPrecompile {
    admin: Address,
    initial_next_proposer: Address,
}

#[derive(Debug)]
enum ProposerControlPrecompileError {
    Fatal(PrecompileError),
    Halt(PrecompileHalt),
}

type ProposerControlPrecompileResult<T> = Result<T, ProposerControlPrecompileError>;

impl ProposerControlPrecompileError {
    fn fatal(err: EvmInternalsError) -> Self {
        Self::Fatal(PrecompileError::Fatal(err.to_string()))
    }

    const fn halt_static(reason: &'static str) -> Self {
        Self::Halt(PrecompileHalt::other_static(reason))
    }
}

impl ProposerControlPrecompile {
    /// Use a lazily-initialized static for the ID since `custom` is not const.
    pub fn id() -> &'static PrecompileId {
        static ID: OnceLock<PrecompileId> = OnceLock::new();
        ID.get_or_init(|| PrecompileId::custom("proposer_control"))
    }

    fn bytecode() -> &'static Bytecode {
        static BYTECODE: OnceLock<Bytecode> = OnceLock::new();
        BYTECODE.get_or_init(|| Bytecode::new_raw(Bytes::from_static(&[0xFE])))
    }

    pub const fn new(admin: Address, initial_next_proposer: Address) -> Self {
        Self {
            admin,
            initial_next_proposer,
        }
    }

    fn map_internals_error(err: EvmInternalsError) -> ProposerControlPrecompileError {
        ProposerControlPrecompileError::fatal(err)
    }

    fn ensure_account_created(
        internals: &mut EvmInternals<'_>,
    ) -> ProposerControlPrecompileResult<()> {
        let account = internals
            .load_account(PROPOSER_CONTROL_PRECOMPILE_ADDR)
            .map_err(Self::map_internals_error)?;

        if account.is_loaded_as_not_existing() {
            // Keep the account non-empty so storage written by the precompile is not pruned.
            internals
                .set_code(PROPOSER_CONTROL_PRECOMPILE_ADDR, Self::bytecode().clone())
                .map_err(Self::map_internals_error)?;
            internals
                .load_account_mut(PROPOSER_CONTROL_PRECOMPILE_ADDR)
                .map_err(Self::map_internals_error)?
                .set_nonce(1);
            internals
                .touch_account(PROPOSER_CONTROL_PRECOMPILE_ADDR)
                .map_err(Self::map_internals_error)?;
        }

        Ok(())
    }

    fn ensure_admin(&self, caller: Address) -> ProposerControlPrecompileResult<()> {
        if caller == self.admin {
            Ok(())
        } else {
            Err(ProposerControlPrecompileError::halt_static(
                "unauthorized caller",
            ))
        }
    }

    fn next_proposer(
        &self,
        internals: &mut EvmInternals<'_>,
    ) -> ProposerControlPrecompileResult<Address> {
        let value = internals
            .sload(PROPOSER_CONTROL_PRECOMPILE_ADDR, NEXT_PROPOSER_SLOT)
            .map_err(Self::map_internals_error)?;
        let raw_value = *value;
        if raw_value.is_zero() {
            return Ok(self.initial_next_proposer);
        }
        Ok(Address::from_word(raw_value.into()))
    }

    fn set_next_proposer(
        internals: &mut EvmInternals<'_>,
        proposer: Address,
    ) -> ProposerControlPrecompileResult<()> {
        if proposer.is_zero() {
            return Err(ProposerControlPrecompileError::halt_static(
                "proposer cannot be zero",
            ));
        }

        Self::ensure_account_created(internals)?;
        let value = U256::from_be_bytes(proposer.into_word().into());
        internals
            .sstore(PROPOSER_CONTROL_PRECOMPILE_ADDR, NEXT_PROPOSER_SLOT, value)
            .map_err(Self::map_internals_error)?;
        internals
            .touch_account(PROPOSER_CONTROL_PRECOMPILE_ADDR)
            .map_err(Self::map_internals_error)?;
        Ok(())
    }
}

impl Precompile for ProposerControlPrecompile {
    fn precompile_id(&self) -> &PrecompileId {
        Self::id()
    }

    fn call(&self, mut input: PrecompileInput<'_>) -> PrecompileResult {
        let caller = input.caller;
        let reservoir = input.reservoir;
        let is_static = input.is_static;

        let decoded = match IProposerControl::IProposerControlCalls::abi_decode(input.data) {
            Ok(v) => v,
            Err(e) => {
                return Ok(PrecompileOutput::halt(
                    PrecompileHalt::other(e.to_string()),
                    reservoir,
                ))
            }
        };
        let internals = input.internals_mut();

        let result = (|| -> ProposerControlPrecompileResult<Bytes> {
            match decoded {
                IProposerControl::IProposerControlCalls::nextProposer(_) => {
                    let proposer = self.next_proposer(internals)?;
                    Ok(proposer.abi_encode().into())
                }
                IProposerControl::IProposerControlCalls::setNextProposer(call) => {
                    if is_static {
                        return Err(ProposerControlPrecompileError::halt_static(
                            "state change during static call",
                        ));
                    }
                    self.ensure_admin(caller)?;
                    Self::set_next_proposer(internals, call.proposer)?;
                    Ok(Bytes::new())
                }
                IProposerControl::IProposerControlCalls::admin(_) => {
                    Ok(self.admin.abi_encode().into())
                }
            }
        })();

        match result {
            Ok(bytes) => Ok(PrecompileOutput::new(0, bytes, reservoir)),
            Err(ProposerControlPrecompileError::Halt(reason)) => {
                Ok(PrecompileOutput::halt(reason, reservoir))
            }
            Err(ProposerControlPrecompileError::Fatal(err)) => Err(err),
        }
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
            BlockEnv, CfgEnv, TxEnv,
        },
        database::{CacheDB, EmptyDB},
        primitives::hardfork::SpecId,
    };

    type TestJournal = Journal<CacheDB<EmptyDB>>;

    const GAS_LIMIT: u64 = 1_000_000;

    fn setup_context() -> (TestJournal, BlockEnv, CfgEnv, TxEnv) {
        let mut journal = Journal::new_with_inner(CacheDB::default(), JournalInner::new());
        journal.inner.set_spec_id(SpecId::PRAGUE);
        let block_env = BlockEnv::default();
        let cfg_env = CfgEnv::default();
        let tx_env = TxEnv::default();
        (journal, block_env, cfg_env, tx_env)
    }

    fn run_call<'a>(
        journal: &'a mut TestJournal,
        block_env: &'a BlockEnv,
        cfg_env: &'a CfgEnv,
        tx_env: &'a TxEnv,
        precompile: &ProposerControlPrecompile,
        caller: Address,
        data: &'a [u8],
        is_static: bool,
    ) -> PrecompileResult {
        let input = PrecompileInput {
            data,
            gas: GAS_LIMIT,
            reservoir: 0,
            caller,
            value: U256::ZERO,
            target_address: PROPOSER_CONTROL_PRECOMPILE_ADDR,
            is_static,
            bytecode_address: PROPOSER_CONTROL_PRECOMPILE_ADDR,
            internals: EvmInternals::new(journal, block_env, cfg_env, tx_env),
        };

        precompile.call(input)
    }

    fn output_bytes(result: PrecompileResult) -> Bytes {
        match result {
            Ok(output) if !output.is_halt() => output.bytes.clone(),
            Ok(output) => panic!("expected successful output, got halt {output:?}"),
            Err(err) => panic!("expected successful output, got fatal error {err:?}"),
        }
    }

    fn assert_halt_message(result: PrecompileResult, expected: &str) {
        match result {
            Ok(output) => {
                assert!(output.is_halt(), "expected halt output, got {output:?}");
                match output.halt_reason() {
                    Some(PrecompileHalt::Other(msg)) => {
                        assert_eq!(msg.as_ref(), expected, "unexpected halt message")
                    }
                    other => panic!("expected custom halt reason, got {other:?}"),
                }
            }
            Err(err) => panic!("expected halting precompile output, got fatal error {err:?}"),
        }
    }

    #[test]
    fn returns_initial_next_proposer_when_storage_unset() {
        let admin = address!("0x0000000000000000000000000000000000000aaa");
        let initial = address!("0x0000000000000000000000000000000000000bbb");
        let precompile = ProposerControlPrecompile::new(admin, initial);
        let (mut journal, block_env, cfg_env, tx_env) = setup_context();

        let data = IProposerControl::nextProposerCall {}.abi_encode();
        let bytes = output_bytes(run_call(
            &mut journal,
            &block_env,
            &cfg_env,
            &tx_env,
            &precompile,
            admin,
            &data,
            true,
        ));
        let decoded = Address::abi_decode(&bytes).expect("address output decodes");

        assert_eq!(decoded, initial);
    }

    #[test]
    fn admin_can_set_next_proposer() {
        let admin = address!("0x0000000000000000000000000000000000000aaa");
        let initial = address!("0x0000000000000000000000000000000000000bbb");
        let next = address!("0x0000000000000000000000000000000000000ccc");
        let precompile = ProposerControlPrecompile::new(admin, initial);
        let (mut journal, block_env, cfg_env, tx_env) = setup_context();

        let set_data = IProposerControl::setNextProposerCall { proposer: next }.abi_encode();
        let result = run_call(
            &mut journal,
            &block_env,
            &cfg_env,
            &tx_env,
            &precompile,
            admin,
            &set_data,
            false,
        );
        output_bytes(result);

        let get_data = IProposerControl::nextProposerCall {}.abi_encode();
        let bytes = output_bytes(run_call(
            &mut journal,
            &block_env,
            &cfg_env,
            &tx_env,
            &precompile,
            admin,
            &get_data,
            true,
        ));
        let decoded = Address::abi_decode(&bytes).expect("address output decodes");

        assert_eq!(decoded, next);
    }

    #[test]
    fn non_admin_cannot_set_next_proposer() {
        let admin = address!("0x0000000000000000000000000000000000000aaa");
        let caller = address!("0x0000000000000000000000000000000000000bbb");
        let next = address!("0x0000000000000000000000000000000000000ccc");
        let precompile = ProposerControlPrecompile::new(admin, Address::ZERO);
        let (mut journal, block_env, cfg_env, tx_env) = setup_context();

        let data = IProposerControl::setNextProposerCall { proposer: next }.abi_encode();
        let result = run_call(
            &mut journal,
            &block_env,
            &cfg_env,
            &tx_env,
            &precompile,
            caller,
            &data,
            false,
        );

        assert_halt_message(result, "unauthorized caller");
    }

    #[test]
    fn rejects_zero_next_proposer() {
        let admin = address!("0x0000000000000000000000000000000000000aaa");
        let precompile = ProposerControlPrecompile::new(admin, Address::ZERO);
        let (mut journal, block_env, cfg_env, tx_env) = setup_context();

        let data = IProposerControl::setNextProposerCall {
            proposer: Address::ZERO,
        }
        .abi_encode();
        let result = run_call(
            &mut journal,
            &block_env,
            &cfg_env,
            &tx_env,
            &precompile,
            admin,
            &data,
            false,
        );

        assert_halt_message(result, "proposer cannot be zero");
    }

    #[test]
    fn rejects_state_change_in_static_call() {
        let admin = address!("0x0000000000000000000000000000000000000aaa");
        let next = address!("0x0000000000000000000000000000000000000bbb");
        let precompile = ProposerControlPrecompile::new(admin, Address::ZERO);
        let (mut journal, block_env, cfg_env, tx_env) = setup_context();

        let data = IProposerControl::setNextProposerCall { proposer: next }.abi_encode();
        let result = run_call(
            &mut journal,
            &block_env,
            &cfg_env,
            &tx_env,
            &precompile,
            admin,
            &data,
            true,
        );

        assert_halt_message(result, "state change during static call");
    }

    #[test]
    fn admin_getter_returns_configured_admin() {
        let admin = address!("0x0000000000000000000000000000000000000aaa");
        let precompile = ProposerControlPrecompile::new(admin, Address::ZERO);
        let (mut journal, block_env, cfg_env, tx_env) = setup_context();

        let data = IProposerControl::adminCall {}.abi_encode();
        let bytes = output_bytes(run_call(
            &mut journal,
            &block_env,
            &cfg_env,
            &tx_env,
            &precompile,
            Address::ZERO,
            &data,
            true,
        ));
        let decoded = Address::abi_decode(&bytes).expect("address output decodes");

        assert_eq!(decoded, admin);
    }
}
