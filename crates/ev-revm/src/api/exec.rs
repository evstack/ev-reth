//! Execution traits for [`EvEvm`], mirroring the Reth mainnet implementations
//! while inserting the EV-specific handler that redirects the base fee.

use crate::{evm::EvEvm, handler::EvHandler};
use alloy_primitives::{Address, Bytes};
use reth_revm::revm::{
    context::{result::ExecResultAndState, ContextSetters},
    context_interface::{
        result::{EVMError, ExecutionResult, HaltReason, InvalidTransaction},
        ContextTr, Database, JournalTr,
    },
    handler::{system_call::SystemCallEvm, EthFrame, Handler, PrecompileProvider, SystemCallTx},
    inspector::{
        InspectCommitEvm, InspectEvm, InspectSystemCallEvm, Inspector, InspectorHandler, JournalExt,
    },
    interpreter::{interpreter::EthInterpreter, InterpreterResult},
    state::EvmState,
    DatabaseCommit, ExecuteCommitEvm, ExecuteEvm,
};

/// Convenience alias for the error type returned by EV executions.
pub type EvError<CTX> = EVMError<<<CTX as ContextTr>::Db as Database>::Error, InvalidTransaction>;

/// Convenience alias for the execution result produced by EV executions.
pub type EvExecutionResult = ExecutionResult<HaltReason>;

impl<CTX, INSP, PRECOMP> ExecuteEvm for EvEvm<CTX, INSP, PRECOMP>
where
    CTX: ContextTr<Journal: JournalTr<State = EvmState>> + ContextSetters,
    <CTX as ContextTr>::Db: Database,
    <CTX as ContextTr>::Journal:
        JournalTr<State = EvmState> + JournalTr<Database = <CTX as ContextTr>::Db>,
    PRECOMP: PrecompileProvider<CTX, Output = InterpreterResult>,
{
    type Tx = <CTX as ContextTr>::Tx;
    type Block = <CTX as ContextTr>::Block;
    type State = EvmState;
    type Error = EvError<CTX>;
    type ExecutionResult = EvExecutionResult;

    fn set_block(&mut self, block: Self::Block) {
        self.inner_mut().ctx.set_block(block);
    }

    fn transact_one(&mut self, tx: Self::Tx) -> Result<Self::ExecutionResult, Self::Error> {
        let redirect = self.redirect();
        let inner = self.inner_mut();
        inner.ctx.set_tx(tx);
        let mut handler = EvHandler::<_, _, EthFrame<EthInterpreter>>::new(redirect);
        handler.run(inner)
    }

    fn finalize(&mut self) -> Self::State {
        self.inner_mut().journal_mut().finalize()
    }

    fn replay(
        &mut self,
    ) -> Result<ExecResultAndState<Self::ExecutionResult, Self::State>, Self::Error> {
        let redirect = self.redirect();
        let inner = self.inner_mut();
        let mut handler = EvHandler::<_, _, EthFrame<EthInterpreter>>::new(redirect);
        handler.run(inner).map(|result| {
            let state = inner.journal_mut().finalize();
            ExecResultAndState::new(result, state)
        })
    }
}

impl<CTX, INSP, PRECOMP> ExecuteCommitEvm for EvEvm<CTX, INSP, PRECOMP>
where
    CTX: ContextTr<Db: DatabaseCommit, Journal: JournalTr<State = EvmState>> + ContextSetters,
    PRECOMP: PrecompileProvider<CTX, Output = InterpreterResult>,
{
    fn commit(&mut self, state: Self::State) {
        self.inner_mut().ctx.db_mut().commit(state);
    }
}

impl<CTX, INSP, PRECOMP> InspectEvm for EvEvm<CTX, INSP, PRECOMP>
where
    CTX: ContextTr<Journal: JournalTr<State = EvmState> + JournalExt> + ContextSetters,
    INSP: Inspector<CTX, EthInterpreter>,
    PRECOMP: PrecompileProvider<CTX, Output = InterpreterResult>,
{
    type Inspector = INSP;

    fn set_inspector(&mut self, inspector: Self::Inspector) {
        self.inner_mut().inspector = inspector;
    }

    fn inspect_one_tx(&mut self, tx: Self::Tx) -> Result<Self::ExecutionResult, Self::Error> {
        let redirect = self.redirect();
        let inner = self.inner_mut();
        inner.ctx.set_tx(tx);
        let mut handler = EvHandler::<_, _, EthFrame<EthInterpreter>>::new(redirect);
        handler.inspect_run(inner)
    }
}

impl<CTX, INSP, PRECOMP> InspectCommitEvm for EvEvm<CTX, INSP, PRECOMP>
where
    CTX: ContextTr<Journal: JournalTr<State = EvmState> + JournalExt, Db: DatabaseCommit>
        + ContextSetters,
    INSP: Inspector<CTX, EthInterpreter>,
    PRECOMP: PrecompileProvider<CTX, Output = InterpreterResult>,
{
}

impl<CTX, INSP, PRECOMP> SystemCallEvm for EvEvm<CTX, INSP, PRECOMP>
where
    CTX: ContextTr<Journal: JournalTr<State = EvmState>, Tx: SystemCallTx> + ContextSetters,
    PRECOMP: PrecompileProvider<CTX, Output = InterpreterResult>,
{
    fn system_call_one_with_caller(
        &mut self,
        caller: Address,
        system_contract_address: Address,
        data: Bytes,
    ) -> Result<Self::ExecutionResult, Self::Error> {
        let redirect = self.redirect();
        let inner = self.inner_mut();
        inner
            .ctx
            .set_tx(<CTX as ContextTr>::Tx::new_system_tx_with_caller(
                caller,
                system_contract_address,
                data,
            ));
        let mut handler = EvHandler::<_, _, EthFrame<EthInterpreter>>::new(redirect);
        handler.run_system_call(inner)
    }
}

impl<CTX, INSP, PRECOMP> InspectSystemCallEvm for EvEvm<CTX, INSP, PRECOMP>
where
    CTX: ContextTr<Journal: JournalTr<State = EvmState> + JournalExt, Tx: SystemCallTx>
        + ContextSetters,
    INSP: Inspector<CTX, EthInterpreter>,
    PRECOMP: PrecompileProvider<CTX, Output = InterpreterResult>,
{
    fn inspect_one_system_call_with_caller(
        &mut self,
        caller: Address,
        system_contract_address: Address,
        data: Bytes,
    ) -> Result<Self::ExecutionResult, Self::Error> {
        let redirect = self.redirect();
        let inner = self.inner_mut();
        inner
            .ctx
            .set_tx(<CTX as ContextTr>::Tx::new_system_tx_with_caller(
                caller,
                system_contract_address,
                data,
            ));
        let mut handler = EvHandler::<_, _, EthFrame<EthInterpreter>>::new(redirect);
        handler.inspect_run_system_call(inner)
    }
}
