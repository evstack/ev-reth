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
