use std::{borrow::Cow, boxed::Box, vec::Vec};

use alloy_consensus::{Transaction, TxReceipt};
use alloy_eips::{eip7685::Requests, Encodable2718};
use alloy_evm::{
    block::{
        state_changes::{balance_increment_state, post_block_balance_increments},
        BlockExecutionError, BlockExecutionResult, BlockExecutor, BlockExecutorFactory,
        BlockExecutorFor, BlockValidationError, ExecutableTx, OnStateHook,
        StateChangePostBlockSource, StateChangeSource, SystemCaller,
    },
    eth::{
        dao_fork, eip6110,
        receipt_builder::{ReceiptBuilder, ReceiptBuilderCtx},
        spec::{EthExecutorSpec, EthSpec},
        EthBlockExecutionCtx,
    },
    Database, EthEvmFactory, Evm, EvmFactory, FromRecoveredTx, FromTxWithEncoded,
};
use alloy_primitives::Log;
use ev_primitives::{Receipt, TransactionSigned};
use reth_codecs::alloy::transaction::Envelope;
use reth_ethereum_forks::EthereumHardfork;
use reth_revm::{
    context_interface::block::Block as BlockEnvTr,
    database_interface::DatabaseCommitExt,
    revm::{context_interface::result::ResultAndState, database::State, DatabaseCommit, Inspector},
};

/// Receipt builder that works with Ev transaction envelopes.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct EvReceiptBuilder;

impl ReceiptBuilder for EvReceiptBuilder {
    type Transaction = TransactionSigned;
    type Receipt = Receipt;

    fn build_receipt<E: Evm>(
        &self,
        ctx: ReceiptBuilderCtx<'_, Self::Transaction, E>,
    ) -> Self::Receipt {
        let ReceiptBuilderCtx {
            tx,
            result,
            cumulative_gas_used,
            ..
        } = ctx;
        Receipt {
            tx_type: tx.tx_type(),
            success: result.is_success(),
            cumulative_gas_used,
            logs: result.into_logs(),
        }
    }
}

/// Block executor for EV transactions.
#[derive(Debug)]
pub struct EvBlockExecutor<'a, Evm, Spec, R: ReceiptBuilder> {
    spec: Spec,
    /// Block execution context (parent hash, withdrawals, ommers, etc.).
    pub ctx: EthBlockExecutionCtx<'a>,
    evm: Evm,
    system_caller: SystemCaller<Spec>,
    receipt_builder: R,
    receipts: Vec<R::Receipt>,
    gas_used: u64,
    blob_gas_used: u64,
}

impl<'a, Evm, Spec, R> EvBlockExecutor<'a, Evm, Spec, R>
where
    Spec: Clone,
    R: ReceiptBuilder,
{
    /// Creates a new block executor with the provided EVM, context, spec, and receipt builder.
    pub fn new(evm: Evm, ctx: EthBlockExecutionCtx<'a>, spec: Spec, receipt_builder: R) -> Self {
        Self {
            evm,
            ctx,
            receipts: Vec::new(),
            gas_used: 0,
            blob_gas_used: 0,
            system_caller: SystemCaller::new(spec.clone()),
            spec,
            receipt_builder,
        }
    }
}

impl<'db, DB, E, Spec, R> BlockExecutor for EvBlockExecutor<'_, E, Spec, R>
where
    DB: Database + 'db,
    E: Evm<
        DB = &'db mut State<DB>,
        Tx: FromRecoveredTx<R::Transaction> + FromTxWithEncoded<R::Transaction>,
    >,
    Spec: EthExecutorSpec,
    R: ReceiptBuilder<Transaction: Transaction + Encodable2718, Receipt: TxReceipt<Log = Log>>,
{
    type Transaction = R::Transaction;
    type Receipt = R::Receipt;
    type Evm = E;

    fn apply_pre_execution_changes(&mut self) -> Result<(), BlockExecutionError> {
        let state_clear_flag = self
            .spec
            .is_spurious_dragon_active_at_block(self.evm.block().number().saturating_to());
        self.evm.db_mut().set_state_clear_flag(state_clear_flag);

        self.system_caller
            .apply_blockhashes_contract_call(self.ctx.parent_hash, &mut self.evm)?;
        self.system_caller
            .apply_beacon_root_contract_call(self.ctx.parent_beacon_block_root, &mut self.evm)?;

        Ok(())
    }

    fn execute_transaction_without_commit(
        &mut self,
        tx: impl ExecutableTx<Self>,
    ) -> Result<ResultAndState<<Self::Evm as Evm>::HaltReason>, BlockExecutionError> {
        let block_available_gas = self.evm.block().gas_limit() - self.gas_used;

        if tx.tx().gas_limit() > block_available_gas {
            return Err(
                BlockValidationError::TransactionGasLimitMoreThanAvailableBlockGas {
                    transaction_gas_limit: tx.tx().gas_limit(),
                    block_available_gas,
                }
                .into(),
            );
        }

        self.evm.transact(&tx).map_err(|err| {
            let hash = tx.tx().trie_hash();
            BlockExecutionError::evm(err, hash)
        })
    }

    fn commit_transaction(
        &mut self,
        output: ResultAndState<<Self::Evm as Evm>::HaltReason>,
        tx: impl ExecutableTx<Self>,
    ) -> Result<u64, BlockExecutionError> {
        let ResultAndState { result, state } = output;

        self.system_caller
            .on_state(StateChangeSource::Transaction(self.receipts.len()), &state);

        let gas_used = result.gas_used();
        self.gas_used += gas_used;

        if self
            .spec
            .is_cancun_active_at_timestamp(self.evm.block().timestamp().saturating_to())
        {
            let tx_blob_gas_used = tx.tx().blob_gas_used().unwrap_or_default();
            self.blob_gas_used = self.blob_gas_used.saturating_add(tx_blob_gas_used);
        }

        self.receipts
            .push(self.receipt_builder.build_receipt(ReceiptBuilderCtx {
                tx: tx.tx(),
                evm: &self.evm,
                result,
                state: &state,
                cumulative_gas_used: self.gas_used,
            }));

        self.evm.db_mut().commit(state);

        Ok(gas_used)
    }

    fn receipts(&self) -> &[Self::Receipt] {
        &self.receipts
    }

    fn finish(
        mut self,
    ) -> Result<(Self::Evm, BlockExecutionResult<R::Receipt>), BlockExecutionError> {
        let requests = if self
            .spec
            .is_prague_active_at_timestamp(self.evm.block().timestamp().saturating_to())
        {
            let deposit_requests =
                eip6110::parse_deposits_from_receipts(&self.spec, &self.receipts)?;

            let mut requests = Requests::default();

            if !deposit_requests.is_empty() {
                requests.push_request_with_type(eip6110::DEPOSIT_REQUEST_TYPE, deposit_requests);
            }

            requests.extend(
                self.system_caller
                    .apply_post_execution_changes(&mut self.evm)?,
            );
            requests
        } else {
            Requests::default()
        };

        let mut balance_increments = post_block_balance_increments(
            &self.spec,
            self.evm.block(),
            self.ctx.ommers,
            self.ctx.withdrawals.as_deref(),
        );

        if self
            .spec
            .ethereum_fork_activation(EthereumHardfork::Dao)
            .transitions_at_block(self.evm.block().number().saturating_to())
        {
            let drained_balance: u128 = self
                .evm
                .db_mut()
                .drain_balances(dao_fork::DAO_HARDFORK_ACCOUNTS)
                .map_err(|_| BlockValidationError::IncrementBalanceFailed)?
                .into_iter()
                .sum();

            *balance_increments
                .entry(dao_fork::DAO_HARDFORK_BENEFICIARY)
                .or_default() += drained_balance;
        }

        self.evm
            .db_mut()
            .increment_balances(balance_increments.clone())
            .map_err(|_| BlockValidationError::IncrementBalanceFailed)?;

        self.system_caller.try_on_state_with(|| {
            balance_increment_state(&balance_increments, self.evm.db_mut()).map(|state| {
                (
                    StateChangeSource::PostBlock(StateChangePostBlockSource::BalanceIncrements),
                    Cow::Owned(state),
                )
            })
        })?;

        Ok((
            self.evm,
            BlockExecutionResult {
                receipts: self.receipts,
                requests,
                gas_used: self.gas_used,
                blob_gas_used: self.blob_gas_used,
            },
        ))
    }

    fn set_state_hook(&mut self, hook: Option<Box<dyn OnStateHook>>) {
        self.system_caller.with_state_hook(hook);
    }

    fn evm_mut(&mut self) -> &mut Self::Evm {
        &mut self.evm
    }

    fn evm(&self) -> &Self::Evm {
        &self.evm
    }
}

/// Block executor factory for EV transactions.
#[derive(Debug, Clone, Default, Copy)]
pub struct EvBlockExecutorFactory<R = EvReceiptBuilder, Spec = EthSpec, EvmFactory = EthEvmFactory>
{
    receipt_builder: R,
    spec: Spec,
    evm_factory: EvmFactory,
}

impl<R, Spec, EvmFactory> EvBlockExecutorFactory<R, Spec, EvmFactory> {
    /// Creates a new EV block executor factory.
    pub const fn new(receipt_builder: R, spec: Spec, evm_factory: EvmFactory) -> Self {
        Self {
            receipt_builder,
            spec,
            evm_factory,
        }
    }

    /// Returns the receipt builder used by the factory.
    pub const fn receipt_builder(&self) -> &R {
        &self.receipt_builder
    }

    /// Returns the spec configuration used by the factory.
    pub const fn spec(&self) -> &Spec {
        &self.spec
    }

    /// Returns the underlying EVM factory.
    pub const fn evm_factory(&self) -> &EvmFactory {
        &self.evm_factory
    }
}

impl<R, Spec, EvmF> BlockExecutorFactory for EvBlockExecutorFactory<R, Spec, EvmF>
where
    R: ReceiptBuilder<Transaction: Transaction + Encodable2718, Receipt: TxReceipt<Log = Log>>
        + Clone,
    Spec: EthExecutorSpec + Clone,
    EvmF: EvmFactory<Tx: FromRecoveredTx<R::Transaction> + FromTxWithEncoded<R::Transaction>>,
    Self: 'static,
{
    type EvmFactory = EvmF;
    type ExecutionCtx<'a> = EthBlockExecutionCtx<'a>;
    type Transaction = R::Transaction;
    type Receipt = R::Receipt;

    fn evm_factory(&self) -> &Self::EvmFactory {
        &self.evm_factory
    }

    fn create_executor<'a, DB, I>(
        &'a self,
        evm: EvmF::Evm<&'a mut State<DB>, I>,
        ctx: Self::ExecutionCtx<'a>,
    ) -> impl BlockExecutorFor<'a, Self, DB, I>
    where
        DB: Database + 'a,
        I: Inspector<EvmF::Context<&'a mut State<DB>>> + 'a,
    {
        EvBlockExecutor::new(evm, ctx, self.spec.clone(), self.receipt_builder.clone())
    }
}
