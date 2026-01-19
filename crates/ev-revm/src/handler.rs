//! Execution handler extensions for EV-specific fee policies.

use crate::{
    base_fee::{BaseFeeRedirect, BaseFeeRedirectError},
    tx_env::{BatchCallsTx, SponsorPayerTx},
};
use reth_revm::{
    inspector::{Inspector, InspectorEvmTr, InspectorHandler},
    revm::{
        context::result::ExecutionResult,
        context::ContextSetters,
        context_interface::{
            result::HaltReason,
            transaction::{AccessListItemTr, TransactionType},
            Block, Cfg, ContextTr, JournalTr, Transaction,
        },
        handler::{
            post_execution, EthFrame, EvmTr, EvmTrError, FrameResult, FrameTr, Handler,
            MainnetHandler,
        },
        interpreter::{
            gas::{calculate_initial_tx_gas, ACCESS_LIST_ADDRESS, ACCESS_LIST_STORAGE_KEY},
            interpreter::EthInterpreter,
            interpreter_action::FrameInit,
            Gas, InitialAndFloorGas,
        },
        primitives::{eip7702, hardfork::SpecId},
        state::{AccountInfo, Bytecode, EvmState},
    },
};
use std::cmp::Ordering;

/// Handler wrapper that mirrors the mainnet handler but applies optional EV-specific policies.
#[derive(Debug, Clone)]
pub struct EvHandler<EVM, ERROR, FRAME> {
    inner: MainnetHandler<EVM, ERROR, FRAME>,
    redirect: Option<BaseFeeRedirect>,
}

impl<EVM, ERROR, FRAME> EvHandler<EVM, ERROR, FRAME> {
    /// Creates a new handler wrapper with the provided redirect policy.
    pub fn new(redirect: Option<BaseFeeRedirect>) -> Self {
        Self {
            inner: MainnetHandler::default(),
            redirect,
        }
    }

    /// Returns the configured redirect policy, if any.
    pub const fn redirect(&self) -> Option<BaseFeeRedirect> {
        self.redirect
    }
}

impl<EVM, ERROR, FRAME> Handler for EvHandler<EVM, ERROR, FRAME>
where
    EVM: EvmTr<
        Context: ContextTr<
            Journal: JournalTr<State = EvmState>,
            Tx: SponsorPayerTx + BatchCallsTx,
        > + ContextSetters,
        Frame = FRAME,
    >,
    <<EVM as EvmTr>::Context as ContextTr>::Tx: Clone,
    ERROR: EvmTrError<EVM>,
    FRAME: FrameTr<FrameResult = FrameResult, FrameInit = FrameInit>,
{
    type Evm = EVM;
    type Error = ERROR;
    type HaltReason = HaltReason;

    fn validate_env(&self, evm: &mut Self::Evm) -> Result<(), Self::Error> {
        self.inner.validate_env(evm)
    }

    fn validate_initial_tx_gas(&self, evm: &Self::Evm) -> Result<InitialAndFloorGas, Self::Error> {
        let ctx = evm.ctx_ref();
        let tx = ctx.tx();
        if let Some(calls) = tx.batch_calls() {
            if calls.is_empty() {
                return Err(Self::Error::from_string(
                    "evnode transaction must include at least one call".into(),
                ));
            }
            if calls.iter().skip(1).any(|call| call.to.is_create()) {
                return Err(Self::Error::from_string(
                    "only the first call may be CREATE".into(),
                ));
            }
            if calls.len() > 1 {
                return validate_batch_initial_tx_gas(tx, calls, ctx.cfg().spec().into(), false)
                    .map_err(From::from);
            }
        }

        self.inner.validate_initial_tx_gas(evm)
    }

    fn load_accounts(&self, evm: &mut Self::Evm) -> Result<(), Self::Error> {
        self.inner.load_accounts(evm)
    }

    fn apply_eip7702_auth_list(&self, evm: &mut Self::Evm) -> Result<u64, Self::Error> {
        self.inner.apply_eip7702_auth_list(evm)
    }

    fn validate_against_state_and_deduct_caller(
        &self,
        evm: &mut Self::Evm,
    ) -> Result<(), Self::Error> {
        let ctx = evm.ctx_mut();
        let tx = ctx.tx();
        let sponsor_invalid = tx.sponsor_signature_invalid();
        let sponsor = tx.sponsor();
        let caller_address = tx.caller();
        let total_value = tx.batch_total_value();
        let is_call = tx.kind().is_call();
        let basefee = ctx.block().basefee() as u128;
        let blob_price = ctx.block().blob_gasprice().unwrap_or_default();
        let is_balance_check_disabled = ctx.cfg().is_balance_check_disabled();
        let is_eip3607_disabled = ctx.cfg().is_eip3607_disabled();
        let is_nonce_check_disabled = ctx.cfg().is_nonce_check_disabled();

        if sponsor_invalid {
            return Err(Self::Error::from_string("invalid sponsor signature".into()));
        }

        let (tx, journal) = ctx.tx_journal_mut();
        if let Some(sponsor) = sponsor {
            {
                let caller = journal.load_account_code(caller_address)?.data;
                validate_account_nonce_and_code(
                    &caller.info,
                    tx,
                    is_eip3607_disabled,
                    is_nonce_check_disabled,
                )?;

                let mut new_caller_balance = caller.info.balance;
                if !is_balance_check_disabled && new_caller_balance < total_value {
                    return Err(reth_revm::revm::context_interface::result::InvalidTransaction::LackOfFundForMaxFee {
                        fee: Box::new(total_value),
                        balance: Box::new(new_caller_balance),
                    }
                    .into());
                }

                new_caller_balance = new_caller_balance.saturating_sub(total_value);
                if is_balance_check_disabled {
                    new_caller_balance = new_caller_balance.max(total_value);
                }

                caller.info.set_balance(new_caller_balance);
                if is_call {
                    caller.info.set_nonce(caller.info.nonce.saturating_add(1));
                }
            }

            let effective_balance_spending =
                tx.effective_balance_spending(basefee, blob_price).expect(
                    "effective balance is always smaller than max balance so it can't overflow",
                );
            let gas_cost = effective_balance_spending - total_value;

            let sponsor_account = journal.load_account_code(sponsor)?.data;
            let sponsor_balance = sponsor_account.info.balance;
            if !is_balance_check_disabled && sponsor_balance < gas_cost {
                return Err(reth_revm::revm::context_interface::result::InvalidTransaction::LackOfFundForMaxFee {
                    fee: Box::new(gas_cost),
                    balance: Box::new(sponsor_balance),
                }
                .into());
            }

            let mut new_sponsor_balance = sponsor_balance.saturating_sub(gas_cost);
            if is_balance_check_disabled {
                new_sponsor_balance = new_sponsor_balance.max(gas_cost);
            }
            sponsor_account.info.set_balance(new_sponsor_balance);
        } else {
            let caller = journal.load_account_code(caller_address)?.data;
            validate_account_nonce_and_code(
                &caller.info,
                tx,
                is_eip3607_disabled,
                is_nonce_check_disabled,
            )?;
            let new_caller_balance = calculate_caller_fee(
                caller.info.balance,
                tx,
                basefee,
                blob_price,
                is_balance_check_disabled,
            )?;
            caller.info.set_balance(new_caller_balance);
            if is_call {
                caller.info.set_nonce(caller.info.nonce.saturating_add(1));
            }
        }

        Ok(())
    }

    fn first_frame_input(
        &mut self,
        evm: &mut Self::Evm,
        gas_limit: u64,
    ) -> Result<FRAME::FrameInit, Self::Error> {
        self.inner.first_frame_input(evm, gas_limit)
    }

    fn execution(
        &mut self,
        evm: &mut Self::Evm,
        init_and_floor_gas: &InitialAndFloorGas,
    ) -> Result<FrameResult, Self::Error> {
        let calls = match evm.ctx().tx().batch_calls() {
            Some(calls) if calls.is_empty() => {
                return Err(Self::Error::from_string(
                    "evnode transaction must include at least one call".into(),
                ));
            }
            Some(calls) if calls.len() > 1 => calls.to_vec(),
            _ => return self.inner.execution(evm, init_and_floor_gas),
        };

        let base_tx = evm.ctx().tx().clone();
        let gas_limit = base_tx.gas_limit();
        let checkpoint = evm.ctx_mut().journal_mut().checkpoint();
        let mut remaining_gas = gas_limit.saturating_sub(init_and_floor_gas.initial_gas);
        let mut total_refunded: i64 = 0;
        let mut last_result: Option<FrameResult> = None;

        for call in calls.iter() {
            let mut call_tx = base_tx.clone();
            call_tx.set_batch_call(call);
            evm.ctx_mut().set_tx(call_tx);
            let first_frame_input = self.inner.first_frame_input(evm, remaining_gas)?;
            let mut frame_result = self.inner.run_exec_loop(evm, first_frame_input)?;
            let instruction_result = frame_result.interpreter_result().result;
            total_refunded = total_refunded.saturating_add(frame_result.gas().refunded());
            remaining_gas = frame_result.gas().remaining();

            if !instruction_result.is_ok() {
                evm.ctx_mut().journal_mut().checkpoint_revert(checkpoint);
                finalize_batch_gas(&mut frame_result, gas_limit, remaining_gas, 0);
                return Ok(frame_result);
            }

            last_result = Some(frame_result);
        }

        evm.ctx_mut().journal_mut().checkpoint_commit();

        let mut frame_result = last_result.expect("batch execution requires at least one call");
        finalize_batch_gas(&mut frame_result, gas_limit, remaining_gas, total_refunded);

        Ok(frame_result)
    }

    fn last_frame_result(
        &mut self,
        evm: &mut Self::Evm,
        frame_result: &mut <FRAME as FrameTr>::FrameResult,
    ) -> Result<(), Self::Error> {
        self.inner.last_frame_result(evm, frame_result)
    }

    fn run_exec_loop(
        &mut self,
        evm: &mut Self::Evm,
        first_frame_input: <FRAME as FrameTr>::FrameInit,
    ) -> Result<FrameResult, Self::Error> {
        self.inner.run_exec_loop(evm, first_frame_input)
    }

    fn eip7623_check_gas_floor(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <FRAME as FrameTr>::FrameResult,
        init_and_floor_gas: InitialAndFloorGas,
    ) {
        self.inner
            .eip7623_check_gas_floor(evm, exec_result, init_and_floor_gas)
    }

    fn refund(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <FRAME as FrameTr>::FrameResult,
        eip7702_refund: i64,
    ) {
        self.inner.refund(evm, exec_result, eip7702_refund)
    }

    fn reimburse_caller(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <FRAME as FrameTr>::FrameResult,
    ) -> Result<(), Self::Error> {
        self.inner.reimburse_caller(evm, exec_result)
    }

    fn reward_beneficiary(
        &self,
        evm: &mut Self::Evm,
        exec_result: &mut <FRAME as FrameTr>::FrameResult,
    ) -> Result<(), Self::Error> {
        let gas = exec_result.gas();
        let spent = gas.spent_sub_refunded();

        if let (Some(redirect), true) = (self.redirect, spent != 0) {
            redirect
                .apply(evm.ctx(), spent)
                .map_err(|BaseFeeRedirectError::Database(err)| Self::Error::from(err))?;
        }

        post_execution::reward_beneficiary(evm.ctx(), gas).map_err(From::from)
    }

    fn execution_result(
        &mut self,
        evm: &mut Self::Evm,
        result: <FRAME as FrameTr>::FrameResult,
    ) -> Result<ExecutionResult<Self::HaltReason>, Self::Error> {
        self.inner.execution_result(evm, result)
    }
}

impl<EVM, ERROR> InspectorHandler for EvHandler<EVM, ERROR, EthFrame<EthInterpreter>>
where
    EVM: InspectorEvmTr<
        Context: ContextTr<Journal: JournalTr<State = EvmState>, Tx: SponsorPayerTx + BatchCallsTx>,
        Frame = EthFrame<EthInterpreter>,
        Inspector: Inspector<<EVM as EvmTr>::Context, EthInterpreter>,
    >,
    <EVM as EvmTr>::Context: ContextSetters,
    <<EVM as EvmTr>::Context as ContextTr>::Tx: Clone,
    ERROR: EvmTrError<EVM>,
{
    type IT = EthInterpreter;
}

fn validate_account_nonce_and_code<Tx>(
    caller_info: &AccountInfo,
    tx: &Tx,
    is_eip3607_disabled: bool,
    is_nonce_check_disabled: bool,
) -> Result<(), reth_revm::revm::context_interface::result::InvalidTransaction>
where
    Tx: Transaction,
{
    if !is_eip3607_disabled {
        let bytecode = match caller_info.code.as_ref() {
            Some(code) => code,
            None => &Bytecode::default(),
        };
        if !bytecode.is_empty() && !bytecode.is_eip7702() {
            return Err(
                reth_revm::revm::context_interface::result::InvalidTransaction::RejectCallerWithCode,
            );
        }
    }

    if !is_nonce_check_disabled {
        let tx_nonce = tx.nonce();
        let state_nonce = caller_info.nonce;
        match tx_nonce.cmp(&state_nonce) {
            Ordering::Greater => {
                return Err(
                    reth_revm::revm::context_interface::result::InvalidTransaction::NonceTooHigh {
                        tx: tx_nonce,
                        state: state_nonce,
                    },
                );
            }
            Ordering::Less => {
                return Err(
                    reth_revm::revm::context_interface::result::InvalidTransaction::NonceTooLow {
                        tx: tx_nonce,
                        state: state_nonce,
                    },
                );
            }
            Ordering::Equal => {}
        }
    }

    Ok(())
}

fn calculate_caller_fee<Tx>(
    balance: reth_revm::revm::primitives::U256,
    tx: &Tx,
    basefee: u128,
    blob_price: u128,
    is_balance_check_disabled: bool,
) -> Result<
    reth_revm::revm::primitives::U256,
    reth_revm::revm::context_interface::result::InvalidTransaction,
>
where
    Tx: Transaction,
{
    let effective_balance_spending = tx
        .effective_balance_spending(basefee, blob_price)
        .expect("effective balance is always smaller than max balance so it can't overflow");
    if !is_balance_check_disabled && balance < effective_balance_spending {
        return Err(
            reth_revm::revm::context_interface::result::InvalidTransaction::LackOfFundForMaxFee {
                fee: Box::new(effective_balance_spending),
                balance: Box::new(balance),
            },
        );
    }

    let gas_balance_spending = effective_balance_spending - tx.value();

    let mut new_balance = balance.saturating_sub(gas_balance_spending);

    if is_balance_check_disabled {
        new_balance = new_balance.max(tx.value());
    }

    Ok(new_balance)
}

fn validate_batch_initial_tx_gas<Tx: Transaction>(
    tx: &Tx,
    calls: &[ev_primitives::Call],
    spec: SpecId,
    is_eip7623_disabled: bool,
) -> Result<InitialAndFloorGas, reth_revm::revm::context_interface::result::InvalidTransaction> {
    let mut initial_gas = 0u64;
    let mut floor_gas = 0u64;

    for call in calls {
        let call_gas =
            calculate_initial_tx_gas(spec, call.input.as_ref(), call.to.is_create(), 0, 0, 0);
        initial_gas = initial_gas.saturating_add(call_gas.initial_gas);
        floor_gas = floor_gas.saturating_add(call_gas.floor_gas);
    }

    let mut accounts = 0u64;
    let mut storages = 0u64;
    if tx.tx_type() != TransactionType::Legacy {
        if let Some(access_list) = tx.access_list() {
            (accounts, storages) = access_list.fold((0u64, 0u64), |(mut acc, mut stor), item| {
                acc = acc.saturating_add(1);
                stor = stor.saturating_add(item.storage_slots().count() as u64);
                (acc, stor)
            });
        }
    }

    initial_gas = initial_gas
        .saturating_add(accounts.saturating_mul(ACCESS_LIST_ADDRESS))
        .saturating_add(storages.saturating_mul(ACCESS_LIST_STORAGE_KEY));

    if spec.is_enabled_in(SpecId::PRAGUE) {
        initial_gas = initial_gas.saturating_add(
            (tx.authorization_list_len() as u64).saturating_mul(eip7702::PER_EMPTY_ACCOUNT_COST),
        );
    } else {
        floor_gas = 0;
    }

    if is_eip7623_disabled {
        floor_gas = 0;
    }

    if initial_gas > tx.gas_limit() {
        return Err(
            reth_revm::revm::context_interface::result::InvalidTransaction::CallGasCostMoreThanGasLimit {
                gas_limit: tx.gas_limit(),
                initial_gas,
            },
        );
    }

    if spec.is_enabled_in(SpecId::PRAGUE) && floor_gas > tx.gas_limit() {
        return Err(
            reth_revm::revm::context_interface::result::InvalidTransaction::GasFloorMoreThanGasLimit {
                gas_floor: floor_gas,
                gas_limit: tx.gas_limit(),
            },
        );
    }

    Ok(InitialAndFloorGas::new(initial_gas, floor_gas))
}

fn finalize_batch_gas(
    frame_result: &mut FrameResult,
    tx_gas_limit: u64,
    remaining_gas: u64,
    refund: i64,
) {
    let instruction_result = frame_result.interpreter_result().result;
    let mut gas = Gas::new_spent(tx_gas_limit);
    if instruction_result.is_ok_or_revert() {
        gas.erase_cost(remaining_gas);
    }
    if instruction_result.is_ok() {
        gas.record_refund(refund);
    }
    *frame_result.gas_mut() = gas;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EvEvm, EvTxEnv, EvTxEvmFactory};
    use alloy_primitives::{address, Address, Bytes, TxKind, B256, U256};
    use ev_primitives::Call;
    use reth_revm::{
        inspector::NoOpInspector,
        revm::{
            context::Context,
            context_interface::result::ExecutionResult,
            context_interface::transaction::{AccessList, AccessListItem, TransactionType},
            database::{CacheDB, EmptyDB},
            handler::{EthFrame, FrameResult},
            interpreter::{CallOutcome, Gas, InstructionResult, InterpreterResult},
            primitives::hardfork::SpecId,
            primitives::KECCAK_EMPTY,
            state::{AccountInfo, EvmState},
        },
        MainContext, State,
    };
    use std::convert::Infallible;

    use reth_revm::revm::context_interface::result::{EVMError, InvalidTransaction};

    type TestContext = Context<BlockEnv, TxEnv, CfgEnv<SpecId>, EmptyDB>;
    type TestEvm = EvEvm<TestContext, NoOpInspector>;
    type TestError = EVMError<Infallible, InvalidTransaction>;
    type TestHandler = EvHandler<TestEvm, TestError, EthFrame<EthInterpreter>>;

    use alloy_evm::{Evm, EvmEnv, EvmFactory};
    use reth_revm::revm::bytecode::Bytecode as RevmBytecode;
    use reth_revm::revm::context::{BlockEnv, CfgEnv, TxEnv};

    const BASE_FEE: u64 = 100;
    const GAS_PRICE: u128 = 200;
    const STORAGE_RUNTIME: [u8; 6] = [0x60, 0x01, 0x60, 0x00, 0x55, 0x00];
    const REVERT_RUNTIME: [u8; 5] = [0x60, 0x00, 0x60, 0x00, 0xfd];

    #[test]
    fn reward_beneficiary_redirects_base_fee_sink() {
        let sink = address!("0x00000000000000000000000000000000000000fe");
        let beneficiary = address!("0x00000000000000000000000000000000000000be");
        let redirect = BaseFeeRedirect::new(sink);

        let (mut evm, handler) = setup_evm(redirect, beneficiary);
        let gas_used = 21_000u64;
        let mut frame_result = make_call_frame(gas_used);

        handler
            .reward_beneficiary(&mut evm, &mut frame_result)
            .expect("reward succeeds");

        let ctx_ref = evm.ctx();
        let journal = ctx_ref.journal();
        let sink_account = journal.account(sink);
        let expected_redirect = U256::from(BASE_FEE) * U256::from(gas_used);
        assert_eq!(sink_account.info.balance, expected_redirect);

        let beneficiary_account = journal.account(beneficiary);
        let tip_per_gas = GAS_PRICE - BASE_FEE as u128;
        let expected_tip = U256::from(tip_per_gas) * U256::from(gas_used);
        assert_eq!(beneficiary_account.info.balance, expected_tip);
    }

    #[test]
    fn reward_beneficiary_skips_redirect_when_no_gas_spent() {
        let sink = address!("0x00000000000000000000000000000000000000fd");
        let beneficiary = address!("0x00000000000000000000000000000000000000bf");
        let redirect = BaseFeeRedirect::new(sink);

        let (mut evm, handler) = setup_evm(redirect, beneficiary);
        let mut frame_result = make_call_frame(0);

        handler
            .reward_beneficiary(&mut evm, &mut frame_result)
            .expect("reward succeeds with zero gas");

        let ctx_ref = evm.ctx();
        let journal = ctx_ref.journal();
        let sink_balance = journal.account(sink).info.balance;
        assert!(sink_balance.is_zero());

        let beneficiary_balance = journal.account(beneficiary).info.balance;
        assert!(beneficiary_balance.is_zero());
    }

    #[test]
    fn batch_initial_gas_sums_calls_and_access_list() {
        let mut tx_env = TxEnv::default();
        tx_env.gas_limit = 1_000_000;
        tx_env.tx_type = TransactionType::Eip1559.into();
        tx_env.access_list = AccessList(vec![AccessListItem {
            address: address!("0x00000000000000000000000000000000000000aa"),
            storage_keys: vec![B256::ZERO, B256::from([0x11; 32])],
        }]);

        let calls = vec![
            Call {
                to: TxKind::Call(address!("0x00000000000000000000000000000000000000bb")),
                value: U256::ZERO,
                input: Bytes::new(),
            },
            Call {
                to: TxKind::Call(address!("0x00000000000000000000000000000000000000cc")),
                value: U256::ZERO,
                input: Bytes::from(vec![0x01, 0x00, 0x02]),
            },
        ];

        let gas_call_1 =
            calculate_initial_tx_gas(SpecId::PRAGUE, calls[0].input.as_ref(), false, 0, 0, 0);
        let gas_call_2 =
            calculate_initial_tx_gas(SpecId::PRAGUE, calls[1].input.as_ref(), false, 0, 0, 0);
        let access_list_cost = ACCESS_LIST_ADDRESS + 2 * ACCESS_LIST_STORAGE_KEY;

        let result = validate_batch_initial_tx_gas(&tx_env, &calls, SpecId::PRAGUE, false)
            .expect("batch gas should validate");

        let expected_initial = gas_call_1
            .initial_gas
            .saturating_add(gas_call_2.initial_gas)
            .saturating_add(access_list_cost);
        let expected_floor = gas_call_1.floor_gas.saturating_add(gas_call_2.floor_gas);

        assert_eq!(result.initial_gas, expected_initial);
        assert_eq!(result.floor_gas, expected_floor);
    }

    #[test]
    fn batch_initial_gas_rejects_when_gas_limit_too_low() {
        let mut tx_env = TxEnv::default();
        tx_env.gas_limit = 10_000;

        let calls = vec![Call {
            to: TxKind::Call(address!("0x00000000000000000000000000000000000000dd")),
            value: U256::ZERO,
            input: Bytes::from(vec![0x11; 64]),
        }];

        let err = validate_batch_initial_tx_gas(&tx_env, &calls, SpecId::CANCUN, false)
            .expect_err("should reject when gas limit is too low");

        assert!(matches!(
            err,
            InvalidTransaction::CallGasCostMoreThanGasLimit { .. }
        ));
    }

    #[test]
    fn batch_execution_reverts_state_on_failure() {
        let caller = address!("0x0000000000000000000000000000000000000aaa");
        let storage_contract = address!("0x0000000000000000000000000000000000000bbb");
        let revert_contract = address!("0x0000000000000000000000000000000000000ccc");

        let mut state = State::builder()
            .with_database(CacheDB::<EmptyDB>::default())
            .with_bundle_update()
            .build();

        state.insert_account(
            caller,
            AccountInfo {
                balance: U256::from(10_000_000_000u64),
                nonce: 0,
                code_hash: KECCAK_EMPTY,
                code: None,
            },
        );

        state.insert_account(
            storage_contract,
            AccountInfo {
                balance: U256::ZERO,
                nonce: 1,
                code_hash: alloy_primitives::keccak256(STORAGE_RUNTIME.as_slice()),
                code: Some(RevmBytecode::new_raw(Bytes::copy_from_slice(
                    STORAGE_RUNTIME.as_slice(),
                ))),
            },
        );

        state.insert_account(
            revert_contract,
            AccountInfo {
                balance: U256::ZERO,
                nonce: 1,
                code_hash: alloy_primitives::keccak256(REVERT_RUNTIME.as_slice()),
                code: Some(RevmBytecode::new_raw(Bytes::copy_from_slice(
                    REVERT_RUNTIME.as_slice(),
                ))),
            },
        );

        let mut evm_env: EvmEnv<SpecId> = EvmEnv::default();
        evm_env.cfg_env.chain_id = 1;
        evm_env.cfg_env.spec = SpecId::CANCUN;
        evm_env.block_env.basefee = 1;
        evm_env.block_env.gas_limit = 30_000_000;
        evm_env.block_env.number = U256::from(1);

        let mut evm = EvTxEvmFactory::default().create_evm(state, evm_env);

        let calls = vec![
            Call {
                to: TxKind::Call(storage_contract),
                value: U256::ZERO,
                input: Bytes::new(),
            },
            Call {
                to: TxKind::Call(revert_contract),
                value: U256::ZERO,
                input: Bytes::new(),
            },
        ];

        let mut tx_env = TxEnv::default();
        tx_env.caller = caller;
        tx_env.gas_limit = 200_000;
        tx_env.gas_price = 1;
        tx_env.gas_priority_fee = Some(1);
        tx_env.chain_id = Some(1);
        tx_env.tx_type = TransactionType::Eip1559.into();

        let tx = EvTxEnv::with_calls(tx_env, calls);

        let result_and_state = evm
            .transact_raw(tx)
            .expect("batch execution should complete");

        assert!(matches!(
            result_and_state.result,
            ExecutionResult::Revert { .. }
        ));

        let state: EvmState = result_and_state.state;
        let storage_account = state
            .get(&storage_contract)
            .expect("storage contract should be loaded");
        if let Some(slot) = storage_account.storage.get(&U256::ZERO) {
            assert!(slot.original_value.is_zero());
            assert!(slot.present_value.is_zero());
            assert!(!slot.is_changed());
        }
    }

    #[test]
    fn batch_execution_commits_state_on_success() {
        let caller = address!("0x0000000000000000000000000000000000000aaa");
        let storage_contract = address!("0x0000000000000000000000000000000000000bbb");

        let mut state = State::builder()
            .with_database(CacheDB::<EmptyDB>::default())
            .with_bundle_update()
            .build();

        state.insert_account(
            caller,
            AccountInfo {
                balance: U256::from(10_000_000_000u64),
                nonce: 0,
                code_hash: KECCAK_EMPTY,
                code: None,
            },
        );

        state.insert_account(
            storage_contract,
            AccountInfo {
                balance: U256::ZERO,
                nonce: 1,
                code_hash: alloy_primitives::keccak256(STORAGE_RUNTIME.as_slice()),
                code: Some(RevmBytecode::new_raw(Bytes::copy_from_slice(
                    STORAGE_RUNTIME.as_slice(),
                ))),
            },
        );

        let mut evm_env: EvmEnv<SpecId> = EvmEnv::default();
        evm_env.cfg_env.chain_id = 1;
        evm_env.cfg_env.spec = SpecId::CANCUN;
        evm_env.block_env.basefee = 1;
        evm_env.block_env.gas_limit = 30_000_000;
        evm_env.block_env.number = U256::from(1);

        let mut evm = EvTxEvmFactory::default().create_evm(state, evm_env);

        let calls = vec![
            Call {
                to: TxKind::Call(storage_contract),
                value: U256::ZERO,
                input: Bytes::new(),
            },
            Call {
                to: TxKind::Call(storage_contract),
                value: U256::ZERO,
                input: Bytes::new(),
            },
        ];

        let mut tx_env = TxEnv::default();
        tx_env.caller = caller;
        tx_env.gas_limit = 200_000;
        tx_env.gas_price = 1;
        tx_env.gas_priority_fee = Some(1);
        tx_env.chain_id = Some(1);
        tx_env.tx_type = TransactionType::Eip1559.into();

        let tx = EvTxEnv::with_calls(tx_env, calls);

        let result_and_state = evm
            .transact_raw(tx)
            .expect("batch execution should complete");

        assert!(matches!(
            result_and_state.result,
            ExecutionResult::Success { .. }
        ));

        let state: EvmState = result_and_state.state;
        let storage_account = state
            .get(&storage_contract)
            .expect("storage contract should be loaded");
        let slot = storage_account
            .storage
            .get(&U256::ZERO)
            .expect("slot 0 should be written");
        assert_eq!(slot.present_value, U256::from(1));
        assert!(slot.is_changed());
    }

    fn setup_evm(redirect: BaseFeeRedirect, beneficiary: Address) -> (TestEvm, TestHandler) {
        let mut ctx = Context::mainnet().with_db(EmptyDB::default());
        ctx.block.basefee = BASE_FEE;
        ctx.block.beneficiary = beneficiary;
        ctx.block.gas_limit = 30_000_000;
        ctx.cfg.spec = SpecId::CANCUN;
        ctx.tx.gas_price = GAS_PRICE;
        ctx.tx.gas_limit = 1_000_000;

        let mut evm = EvEvm::new(ctx, NoOpInspector, Some(redirect));
        {
            let journal = evm.ctx_mut().journal_mut();
            journal.load_account(redirect.fee_sink()).unwrap();
            journal.load_account(beneficiary).unwrap();
        }

        let handler: TestHandler = EvHandler::new(Some(redirect));
        (evm, handler)
    }

    fn make_call_frame(gas_used: u64) -> FrameResult {
        let gas = Gas::new_spent(gas_used);
        let interpreter_result =
            InterpreterResult::new(InstructionResult::Return, Bytes::new(), gas);
        FrameResult::Call(CallOutcome::new(interpreter_result, 0..0))
    }
}
