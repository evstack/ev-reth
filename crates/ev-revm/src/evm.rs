//! EV-specific EVM wrapper that installs the base-fee redirect handler.

use crate::{base_fee::BaseFeeRedirect, deploy::DeployAllowlistSettings, tx_env::EvTxEnv};
use alloy_evm::{Evm as AlloyEvm, EvmEnv};
use alloy_primitives::{Address, Bytes};
use reth_revm::{
    revm::{
        context::{BlockEnv, CfgEnv, ContextError, ContextSetters, Evm, FrameStack, TxEnv},
        context_interface::{
            result::{EVMError, HaltReason, InvalidTransaction, ResultAndState},
            ContextTr, Database, JournalTr,
        },
        handler::{
            instructions::EthInstructions, EthFrame, EthPrecompiles, EvmTr, FrameInitOrResult,
            FrameTr, ItemOrResult, PrecompileProvider,
        },
        inspector::{InspectEvm, InspectSystemCallEvm, Inspector, InspectorEvmTr},
        interpreter::{interpreter::EthInterpreter, InterpreterResult},
        primitives::hardfork::SpecId,
        state::EvmState,
        ExecuteEvm, SystemCallEvm,
    },
    Context,
};
use revm_inspector::JournalExt;
use std::ops::{Deref, DerefMut};

/// Convenience alias matching the stock mainnet EVM signature.
pub type DefaultEvEvm<CTX, INSP = ()> = EvEvm<CTX, INSP, EthPrecompiles>;

/// Wrapper around the stock mainnet EVM that installs the EV handler for each transaction.
#[derive(Debug)]
pub struct EvEvm<CTX, INSP, PRECOMP = EthPrecompiles> {
    inner: Evm<CTX, INSP, EthInstructions<EthInterpreter, CTX>, PRECOMP, EthFrame<EthInterpreter>>,
    redirect: Option<BaseFeeRedirect>,
    deploy_allowlist: Option<DeployAllowlistSettings>,
    inspect: bool,
}

impl<CTX, INSP, P> EvEvm<CTX, INSP, P>
where
    CTX: ContextTr + ContextSetters,
    P: Default,
{
    /// Creates a new wrapper configured with the provided redirect policy.
    pub fn new(ctx: CTX, inspector: INSP, redirect: Option<BaseFeeRedirect>) -> Self {
        Self {
            inner: Evm {
                ctx,
                inspector,
                instruction: EthInstructions::new_mainnet(),
                precompiles: P::default(),
                frame_stack: FrameStack::new(),
            },
            redirect,
            deploy_allowlist: None,
            inspect: false,
        }
    }
}

impl<CTX, INSP, P> EvEvm<CTX, INSP, P> {
    /// Wraps an existing EVM instance with the redirect policy.
    pub fn from_inner<T>(
        inner: T,
        redirect: Option<BaseFeeRedirect>,
        deploy_allowlist: Option<DeployAllowlistSettings>,
        inspect: bool,
    ) -> Self
    where
        T: IntoRevmEvm<CTX, INSP, P>,
    {
        Self {
            inner: inner.into_revm_evm(),
            redirect,
            deploy_allowlist,
            inspect,
        }
    }

    /// Converts the wrapper back into the underlying EVM.
    pub fn into_inner(
        self,
    ) -> Evm<CTX, INSP, EthInstructions<EthInterpreter, CTX>, P, EthFrame<EthInterpreter>> {
        self.inner
    }

    /// Returns the configured base-fee redirect policy.
    pub const fn redirect(&self) -> Option<BaseFeeRedirect> {
        self.redirect
    }

    /// Returns the configured deploy allowlist settings, if any.
    pub fn deploy_allowlist(&self) -> Option<DeployAllowlistSettings> {
        self.deploy_allowlist.clone()
    }

    /// Allows adjusting the precompiles map while preserving redirect configuration.
    pub fn with_precompiles<OP>(self, precompiles: OP) -> EvEvm<CTX, INSP, OP> {
        EvEvm {
            inner: self.inner.with_precompiles(precompiles),
            redirect: self.redirect,
            deploy_allowlist: self.deploy_allowlist,
            inspect: self.inspect,
        }
    }

    /// Allows swapping the inspector while preserving redirect configuration.
    pub fn with_inspector<OINSP>(self, inspector: OINSP) -> EvEvm<CTX, OINSP, P> {
        EvEvm {
            inner: self.inner.with_inspector(inspector),
            redirect: self.redirect,
            deploy_allowlist: self.deploy_allowlist,
            inspect: self.inspect,
        }
    }

    /// Exposes a mutable reference to the wrapped `Evm`.
    pub(crate) fn inner_mut(
        &mut self,
    ) -> &mut Evm<CTX, INSP, EthInstructions<EthInterpreter, CTX>, P, EthFrame<EthInterpreter>>
    {
        &mut self.inner
    }

    /// Returns whether inspection is enabled.
    pub const fn inspect_enabled(&self) -> bool {
        self.inspect
    }
}

/// Helper trait for converting EVM wrappers back into the core `revm` EVM type.
pub trait IntoRevmEvm<CTX, INSP, PRECOMP> {
    /// Consumes the wrapper and returns the underlying `revm` EVM instance.
    fn into_revm_evm(
        self,
    ) -> Evm<CTX, INSP, EthInstructions<EthInterpreter, CTX>, PRECOMP, EthFrame<EthInterpreter>>;
}

impl<CTX, INSP, PRECOMP> IntoRevmEvm<CTX, INSP, PRECOMP>
    for Evm<CTX, INSP, EthInstructions<EthInterpreter, CTX>, PRECOMP, EthFrame<EthInterpreter>>
{
    fn into_revm_evm(self) -> Self {
        self
    }
}

impl<DB, I, PRECOMP> IntoRevmEvm<Context<BlockEnv, TxEnv, CfgEnv<SpecId>, DB>, I, PRECOMP>
    for alloy_evm::eth::EthEvm<DB, I, PRECOMP>
where
    DB: alloy_evm::Database,
    I: Inspector<Context<BlockEnv, TxEnv, CfgEnv<SpecId>, DB>, EthInterpreter>,
    PRECOMP: PrecompileProvider<
        Context<BlockEnv, TxEnv, CfgEnv<SpecId>, DB>,
        Output = InterpreterResult,
    >,
{
    fn into_revm_evm(
        self,
    ) -> Evm<
        Context<BlockEnv, TxEnv, CfgEnv<SpecId>, DB>,
        I,
        EthInstructions<EthInterpreter, Context<BlockEnv, TxEnv, CfgEnv<SpecId>, DB>>,
        PRECOMP,
        EthFrame<EthInterpreter>,
    > {
        self.into_inner()
    }
}

impl<CTX, INSP, PRECOMP> Deref for EvEvm<CTX, INSP, PRECOMP> {
    type Target =
        Evm<CTX, INSP, EthInstructions<EthInterpreter, CTX>, PRECOMP, EthFrame<EthInterpreter>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<CTX, INSP, PRECOMP> DerefMut for EvEvm<CTX, INSP, PRECOMP> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<CTX, INSP, PRECOMP> InspectorEvmTr for EvEvm<CTX, INSP, PRECOMP>
where
    CTX: ContextTr<Journal: JournalTr<State = EvmState> + JournalExt> + ContextSetters,
    PRECOMP: PrecompileProvider<CTX, Output = InterpreterResult>,
    INSP: Inspector<CTX, EthInterpreter>,
{
    type Inspector = INSP;

    fn inspector(&mut self) -> &mut Self::Inspector {
        &mut self.inner.inspector
    }

    fn ctx_inspector(&mut self) -> (&mut Self::Context, &mut Self::Inspector) {
        (&mut self.inner.ctx, &mut self.inner.inspector)
    }

    fn ctx_inspector_frame(
        &mut self,
    ) -> (&mut Self::Context, &mut Self::Inspector, &mut Self::Frame) {
        (
            &mut self.inner.ctx,
            &mut self.inner.inspector,
            self.inner.frame_stack.get(),
        )
    }

    fn ctx_inspector_frame_instructions(
        &mut self,
    ) -> (
        &mut Self::Context,
        &mut Self::Inspector,
        &mut Self::Frame,
        &mut Self::Instructions,
    ) {
        (
            &mut self.inner.ctx,
            &mut self.inner.inspector,
            self.inner.frame_stack.get(),
            &mut self.inner.instruction,
        )
    }
}

impl<CTX, INSP, PRECOMP> EvmTr for EvEvm<CTX, INSP, PRECOMP>
where
    CTX: ContextTr,
    PRECOMP: PrecompileProvider<CTX, Output = InterpreterResult>,
    INSP: Inspector<CTX, EthInterpreter>,
{
    type Context = CTX;
    type Instructions = EthInstructions<EthInterpreter, CTX>;
    type Precompiles = PRECOMP;
    type Frame = EthFrame<EthInterpreter>;

    fn ctx(&mut self) -> &mut Self::Context {
        &mut self.inner.ctx
    }

    fn ctx_ref(&self) -> &Self::Context {
        &self.inner.ctx
    }

    fn ctx_instructions(&mut self) -> (&mut Self::Context, &mut Self::Instructions) {
        (&mut self.inner.ctx, &mut self.inner.instruction)
    }

    fn ctx_precompiles(&mut self) -> (&mut Self::Context, &mut Self::Precompiles) {
        (&mut self.inner.ctx, &mut self.inner.precompiles)
    }

    fn frame_stack(&mut self) -> &mut FrameStack<Self::Frame> {
        &mut self.inner.frame_stack
    }

    fn frame_init(
        &mut self,
        frame_input: <Self::Frame as FrameTr>::FrameInit,
    ) -> Result<
        ItemOrResult<&mut Self::Frame, <Self::Frame as FrameTr>::FrameResult>,
        ContextError<<<Self::Context as ContextTr>::Db as Database>::Error>,
    > {
        self.inner.frame_init(frame_input)
    }

    fn frame_run(
        &mut self,
    ) -> Result<
        FrameInitOrResult<Self::Frame>,
        ContextError<<<Self::Context as ContextTr>::Db as Database>::Error>,
    > {
        self.inner.frame_run()
    }

    fn frame_return_result(
        &mut self,
        result: <Self::Frame as FrameTr>::FrameResult,
    ) -> Result<
        Option<<Self::Frame as FrameTr>::FrameResult>,
        ContextError<<<Self::Context as ContextTr>::Db as Database>::Error>,
    > {
        self.inner.frame_return_result(result)
    }
}

impl<DB, INSP, PRECOMP> AlloyEvm
    for EvEvm<Context<BlockEnv, TxEnv, CfgEnv<SpecId>, DB>, INSP, PRECOMP>
where
    DB: alloy_evm::Database,
    INSP: Inspector<Context<BlockEnv, TxEnv, CfgEnv<SpecId>, DB>, EthInterpreter>,
    PRECOMP: PrecompileProvider<
        Context<BlockEnv, TxEnv, CfgEnv<SpecId>, DB>,
        Output = InterpreterResult,
    >,
{
    type DB = DB;
    type Tx = TxEnv;
    type Error = EVMError<DB::Error, InvalidTransaction>;
    type HaltReason = HaltReason;
    type Spec = SpecId;
    type Precompiles = PRECOMP;
    type Inspector = INSP;

    fn block(&self) -> &BlockEnv {
        &self.inner.ctx.block
    }

    fn chain_id(&self) -> u64 {
        self.inner.ctx.cfg.chain_id
    }

    fn transact_raw(
        &mut self,
        tx: Self::Tx,
    ) -> Result<ResultAndState<Self::HaltReason>, Self::Error> {
        if self.inspect {
            InspectEvm::inspect_tx(self, tx)
        } else {
            ExecuteEvm::transact(self, tx)
        }
        .map(|res| ResultAndState::new(res.result, res.state))
    }

    fn transact_system_call(
        &mut self,
        caller: Address,
        contract: Address,
        data: Bytes,
    ) -> Result<ResultAndState<Self::HaltReason>, Self::Error> {
        if self.inspect {
            InspectSystemCallEvm::inspect_system_call_with_caller(self, caller, contract, data)
        } else {
            SystemCallEvm::system_call_with_caller(self, caller, contract, data)
        }
        .map(|res| ResultAndState::new(res.result, res.state))
    }

    fn finish(self) -> (Self::DB, EvmEnv<Self::Spec>) {
        let Self { inner, .. } = self;
        let Context {
            block,
            cfg,
            journaled_state,
            ..
        } = inner.ctx;
        (
            journaled_state.database,
            EvmEnv {
                block_env: block,
                cfg_env: cfg,
            },
        )
    }

    fn set_inspector_enabled(&mut self, enabled: bool) {
        self.inspect = enabled;
    }

    fn components(&self) -> (&Self::DB, &Self::Inspector, &Self::Precompiles) {
        (
            &self.inner.ctx.journaled_state.database,
            &self.inner.inspector,
            &self.inner.precompiles,
        )
    }

    fn components_mut(&mut self) -> (&mut Self::DB, &mut Self::Inspector, &mut Self::Precompiles) {
        (
            &mut self.inner.ctx.journaled_state.database,
            &mut self.inner.inspector,
            &mut self.inner.precompiles,
        )
    }
}

/// Implementation of [`AlloyEvm`] for the EV-specific EVM context.
///
/// Generic parameters:
/// - `DB`: State database for reading/writing accounts and storage
/// - `INSP`: Inspector for tracing and debugging EVM execution
/// - `PRECOMP`: Provider for precompiled contracts (e.g., ecrecover, sha256)
impl<DB, INSP, PRECOMP> AlloyEvm
    for EvEvm<Context<BlockEnv, EvTxEnv, CfgEnv<SpecId>, DB>, INSP, PRECOMP>
where
    DB: alloy_evm::Database,
    INSP: Inspector<Context<BlockEnv, EvTxEnv, CfgEnv<SpecId>, DB>, EthInterpreter>,
    PRECOMP: PrecompileProvider<
        Context<BlockEnv, EvTxEnv, CfgEnv<SpecId>, DB>,
        Output = InterpreterResult,
    >,
{
    type DB = DB;
    type Tx = EvTxEnv;
    type Error = EVMError<DB::Error, InvalidTransaction>;
    type HaltReason = HaltReason;
    type Spec = SpecId;
    type Precompiles = PRECOMP;
    type Inspector = INSP;

    fn block(&self) -> &BlockEnv {
        &self.inner.ctx.block
    }

    fn chain_id(&self) -> u64 {
        self.inner.ctx.cfg.chain_id
    }

    fn transact_raw(
        &mut self,
        tx: Self::Tx,
    ) -> Result<ResultAndState<Self::HaltReason>, Self::Error> {
        if self.inspect {
            InspectEvm::inspect_tx(self, tx)
        } else {
            ExecuteEvm::transact(self, tx)
        }
        .map(|res| ResultAndState::new(res.result, res.state))
    }

    fn transact_system_call(
        &mut self,
        caller: Address,
        contract: Address,
        data: Bytes,
    ) -> Result<ResultAndState<Self::HaltReason>, Self::Error> {
        if self.inspect {
            InspectSystemCallEvm::inspect_system_call_with_caller(self, caller, contract, data)
        } else {
            SystemCallEvm::system_call_with_caller(self, caller, contract, data)
        }
        .map(|res| ResultAndState::new(res.result, res.state))
    }

    fn finish(self) -> (Self::DB, EvmEnv<Self::Spec>) {
        let Self { inner, .. } = self;
        let Context {
            block,
            cfg,
            journaled_state,
            ..
        } = inner.ctx;
        (
            journaled_state.database,
            EvmEnv {
                block_env: block,
                cfg_env: cfg,
            },
        )
    }

    fn set_inspector_enabled(&mut self, enabled: bool) {
        self.inspect = enabled;
    }

    fn components(&self) -> (&Self::DB, &Self::Inspector, &Self::Precompiles) {
        (
            &self.inner.ctx.journaled_state.database,
            &self.inner.inspector,
            &self.inner.precompiles,
        )
    }

    fn components_mut(&mut self) -> (&mut Self::DB, &mut Self::Inspector, &mut Self::Precompiles) {
        (
            &mut self.inner.ctx.journaled_state.database,
            &mut self.inner.inspector,
            &mut self.inner.precompiles,
        )
    }
}
