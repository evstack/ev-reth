//! Execution handler extensions for EV-specific fee policies.

use crate::base_fee::{BaseFeeRedirect, BaseFeeRedirectError};
use reth_revm::{
    inspector::{Inspector, InspectorEvmTr, InspectorHandler},
    revm::{
        context::result::ExecutionResult,
        context_interface::{result::HaltReason, ContextTr, JournalTr},
        handler::{
            post_execution, EthFrame, EvmTr, EvmTrError, FrameResult, FrameTr, Handler,
            MainnetHandler,
        },
        interpreter::{
            interpreter::EthInterpreter, interpreter_action::FrameInit, InitialAndFloorGas,
        },
        state::EvmState,
    },
};

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
    EVM: EvmTr<Context: ContextTr<Journal: JournalTr<State = EvmState>>, Frame = FRAME>,
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
        self.inner.validate_against_state_and_deduct_caller(evm)
    }

    fn first_frame_input(
        &mut self,
        evm: &mut Self::Evm,
        gas_limit: u64,
    ) -> Result<FRAME::FrameInit, Self::Error> {
        self.inner.first_frame_input(evm, gas_limit)
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
        Context: ContextTr<Journal: JournalTr<State = EvmState>>,
        Frame = EthFrame<EthInterpreter>,
        Inspector: Inspector<<EVM as EvmTr>::Context, EthInterpreter>,
    >,
    ERROR: EvmTrError<EVM>,
{
    type IT = EthInterpreter;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EvEvm;
    use alloy_primitives::{address, Address, Bytes, U256};
    use reth_revm::{
        inspector::NoOpInspector,
        revm::{
            context::Context,
            database::EmptyDB,
            handler::{EthFrame, FrameResult},
            interpreter::{CallOutcome, Gas, InstructionResult, InterpreterResult},
            primitives::hardfork::SpecId,
        },
        MainContext,
    };
    use std::convert::Infallible;

    use reth_revm::revm::context_interface::result::{EVMError, InvalidTransaction};

    type TestContext = Context<BlockEnv, TxEnv, CfgEnv<SpecId>, EmptyDB>;
    type TestEvm = EvEvm<TestContext, NoOpInspector>;
    type TestError = EVMError<Infallible, InvalidTransaction>;
    type TestHandler = EvHandler<TestEvm, TestError, EthFrame<EthInterpreter>>;

    use reth_revm::revm::context::{BlockEnv, CfgEnv, TxEnv};

    const BASE_FEE: u64 = 100;
    const GAS_PRICE: u128 = 200;

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
