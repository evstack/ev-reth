use alloy_consensus::Header;
use alloy_evm::{
    block::{BlockExecutorFactory, BlockExecutorFor, ExecutableTx},
    Database as EvmDatabase,
    eth::EthBlockExecutionCtx,
    Evm, EvmFactory,
};
use alloy_primitives::{address, Address, U256};
use alloy_rpc_types::engine::ExecutionData;
use reth_evm::{
    execute::BlockExecutionError, ConfigureEngineEvm, ConfigureEvm, EvmEnv, EvmEnvFor,
    ExecutableTxIterator, ExecutionCtxFor, NextBlockEnvAttributes,
};
use reth_evm_ethereum::EthBlockAssembler;
use reth_execution_types::BlockExecutionResult;
use reth_primitives_traits::{SealedBlock, SealedHeader};
use reth_revm::{
    database_interface::{Database, DatabaseCommit},
    inspector::Inspector,
    state::{Account, AccountStatus, EvmStorageSlot},
    State,
};
use revm::context::result::ResultAndState;
use tracing::info;

use crate::executor::EvolveEvmConfig;

pub const WTIA_ADDRESS: Address = address!("00000000000000000000000000000000Ce1e571A");

pub const NAME_SLOT: U256 = U256::ZERO;
pub const SYMBOL_SLOT: U256 = U256::from_limbs([1, 0, 0, 0]);
pub const DECIMALS_SLOT: U256 = U256::from_limbs([2, 0, 0, 0]);

pub const NAME_VALUE: U256 = U256::from_be_bytes([
    0x57, 0x72, 0x61, 0x70, 0x70, 0x65, 0x64, 0x20,
    0x54, 0x49, 0x41, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x16,
]);

pub const SYMBOL_VALUE: U256 = U256::from_be_bytes([
    0x57, 0x54, 0x49, 0x41, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
]);

pub const DECIMALS_VALUE: U256 = U256::from_limbs([18, 0, 0, 0]);

pub fn maybe_apply_eden_hardfork<DB: Database + DatabaseCommit>(
    db: &mut DB,
    block_number: U256,
    hardfork_height: Option<u64>,
) -> Result<(), BlockExecutionError> {
    if let Some(h) = hardfork_height {
        if block_number == U256::from(h) {
            apply_eden_storage_changes(db)?;
        }
    }
    Ok(())
}

pub fn apply_eden_storage_changes<DB: Database + DatabaseCommit>(
    db: &mut DB,
) -> Result<(), BlockExecutionError> {
    use alloy_primitives::map::HashMap;

    let info = db
        .basic(WTIA_ADDRESS)
        .map_err(|_| BlockExecutionError::msg("failed to load WTIA account"))?
        .unwrap_or_default();

    let storage = HashMap::from_iter([
        (NAME_SLOT, EvmStorageSlot::new_changed(U256::ZERO, NAME_VALUE, 0)),
        (SYMBOL_SLOT, EvmStorageSlot::new_changed(U256::ZERO, SYMBOL_VALUE, 0)),
        (DECIMALS_SLOT, EvmStorageSlot::new_changed(U256::ZERO, DECIMALS_VALUE, 0)),
    ]);

    let account = Account {
        info,
        transaction_id: 0,
        storage,
        status: AccountStatus::Touched,
    };

    db.commit(HashMap::from_iter([(WTIA_ADDRESS, account)]));

    info!(
        target: "ev-reth::hardfork",
        address = ?WTIA_ADDRESS,
        "Applied Eden WTIA storage hardfork"
    );

    Ok(())
}

#[derive(Debug)]
pub struct EdenBlockExecutor<E> {
    inner: E,
    hardfork_height: Option<u64>,
}

impl<E> alloy_evm::block::BlockExecutor for EdenBlockExecutor<E>
where
    E: alloy_evm::block::BlockExecutor,
    E::Evm: Evm,
    <E::Evm as Evm>::DB: Database + DatabaseCommit,
{
    type Transaction = E::Transaction;
    type Receipt = E::Receipt;
    type Evm = E::Evm;

    fn apply_pre_execution_changes(&mut self) -> Result<(), BlockExecutionError> {
        self.inner.apply_pre_execution_changes()
    }

    fn execute_transaction_without_commit(
        &mut self,
        tx: impl ExecutableTx<Self>,
    ) -> Result<
        ResultAndState<<Self::Evm as Evm>::HaltReason>,
        BlockExecutionError,
    > {
        self.inner.execute_transaction_without_commit(tx)
    }

    fn commit_transaction(
        &mut self,
        output: ResultAndState<<Self::Evm as Evm>::HaltReason>,
        tx: impl ExecutableTx<Self>,
    ) -> Result<u64, BlockExecutionError> {
        self.inner.commit_transaction(output, tx)
    }

    fn finish(
        mut self,
    ) -> Result<(Self::Evm, BlockExecutionResult<Self::Receipt>), BlockExecutionError> {
        let block_number = self.inner.evm().block().number;
        maybe_apply_eden_hardfork(
            self.inner.evm_mut().db_mut(),
            block_number,
            self.hardfork_height,
        )?;
        self.inner.finish()
    }

    fn set_state_hook(&mut self, hook: Option<Box<dyn alloy_evm::block::OnStateHook>>) {
        self.inner.set_state_hook(hook);
    }

    fn evm_mut(&mut self) -> &mut Self::Evm {
        self.inner.evm_mut()
    }

    fn evm(&self) -> &Self::Evm {
        self.inner.evm()
    }
}

#[derive(Debug, Clone)]
pub struct EdenBlockExecutorFactory<F> {
    inner: F,
    hardfork_height: Option<u64>,
}

impl<F> EdenBlockExecutorFactory<F> {
    pub const fn new(inner: F, hardfork_height: Option<u64>) -> Self {
        Self {
            inner,
            hardfork_height,
        }
    }
}

impl<F> BlockExecutorFactory for EdenBlockExecutorFactory<F>
where
    F: BlockExecutorFactory,
{
    type EvmFactory = F::EvmFactory;
    type ExecutionCtx<'a> = F::ExecutionCtx<'a>;
    type Transaction = F::Transaction;
    type Receipt = F::Receipt;

    fn evm_factory(&self) -> &Self::EvmFactory {
        self.inner.evm_factory()
    }

    fn create_executor<'a, DB, I>(
        &'a self,
        evm: <Self::EvmFactory as EvmFactory>::Evm<&'a mut State<DB>, I>,
        ctx: Self::ExecutionCtx<'a>,
    ) -> impl BlockExecutorFor<'a, Self, DB, I>
    where
        DB: EvmDatabase + 'a,
        I: Inspector<<Self::EvmFactory as EvmFactory>::Context<&'a mut State<DB>>> + 'a,
    {
        EdenBlockExecutor {
            inner: self.inner.create_executor(evm, ctx),
            hardfork_height: self.hardfork_height,
        }
    }
}

type InnerExecutorFactory =
    <EvolveEvmConfig as ConfigureEvm>::BlockExecutorFactory;

#[derive(Debug, Clone)]
pub struct EdenEvmConfig {
    inner: EvolveEvmConfig,
    eden_factory: EdenBlockExecutorFactory<InnerExecutorFactory>,
}

impl EdenEvmConfig {
    pub fn new(inner: EvolveEvmConfig, hardfork_height: Option<u64>) -> Self {
        let eden_factory =
            EdenBlockExecutorFactory::new(inner.executor_factory.clone(), hardfork_height);
        Self {
            inner,
            eden_factory,
        }
    }
}

impl ConfigureEvm for EdenEvmConfig {
    type Primitives = <EvolveEvmConfig as ConfigureEvm>::Primitives;
    type Error = <EvolveEvmConfig as ConfigureEvm>::Error;
    type NextBlockEnvCtx = NextBlockEnvAttributes;
    type BlockExecutorFactory = EdenBlockExecutorFactory<InnerExecutorFactory>;
    type BlockAssembler = EthBlockAssembler<reth_chainspec::ChainSpec>;

    fn block_executor_factory(&self) -> &Self::BlockExecutorFactory {
        &self.eden_factory
    }

    fn block_assembler(&self) -> &Self::BlockAssembler {
        self.inner.block_assembler()
    }

    fn evm_env(&self, header: &Header) -> Result<EvmEnv, Self::Error> {
        self.inner.evm_env(header)
    }

    fn next_evm_env(
        &self,
        parent: &Header,
        attributes: &NextBlockEnvAttributes,
    ) -> Result<EvmEnv, Self::Error> {
        self.inner.next_evm_env(parent, attributes)
    }

    fn context_for_block<'a>(
        &self,
        block: &'a SealedBlock<reth_ethereum_primitives::Block>,
    ) -> Result<EthBlockExecutionCtx<'a>, Self::Error> {
        self.inner.context_for_block(block)
    }

    fn context_for_next_block(
        &self,
        parent: &SealedHeader,
        attributes: Self::NextBlockEnvCtx,
    ) -> Result<ExecutionCtxFor<'_, Self>, Self::Error> {
        self.inner.context_for_next_block(parent, attributes)
    }
}

impl ConfigureEngineEvm<ExecutionData> for EdenEvmConfig {
    fn evm_env_for_payload(&self, payload: &ExecutionData) -> EvmEnvFor<Self> {
        self.inner.evm_env_for_payload(payload)
    }

    fn context_for_payload<'a>(&self, payload: &'a ExecutionData) -> ExecutionCtxFor<'a, Self> {
        self.inner.context_for_payload(payload)
    }

    fn tx_iterator_for_payload(&self, payload: &ExecutionData) -> impl ExecutableTxIterator<Self> {
        self.inner.tx_iterator_for_payload(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use revm::database::CacheDB;
    use revm::database_interface::EmptyDB;

    #[test]
    fn apply_eden_storage_changes_writes_correct_values() {
        let mut db = CacheDB::<EmptyDB>::default();

        apply_eden_storage_changes(&mut db).expect("should succeed");

        let name = Database::storage(&mut db, WTIA_ADDRESS, NAME_SLOT)
            .expect("storage read should succeed");
        assert_eq!(name, NAME_VALUE);

        let symbol = Database::storage(&mut db, WTIA_ADDRESS, SYMBOL_SLOT)
            .expect("storage read should succeed");
        assert_eq!(symbol, SYMBOL_VALUE);

        let decimals = Database::storage(&mut db, WTIA_ADDRESS, DECIMALS_SLOT)
            .expect("storage read should succeed");
        assert_eq!(decimals, DECIMALS_VALUE);
    }

    #[test]
    fn apply_eden_storage_changes_works_on_missing_account() {
        let mut db = CacheDB::<EmptyDB>::default();

        apply_eden_storage_changes(&mut db).expect("should succeed");

        let name = Database::storage(&mut db, WTIA_ADDRESS, NAME_SLOT)
            .expect("storage read should succeed");
        assert_eq!(name, NAME_VALUE);
    }

    #[test]
    fn hardfork_applies_at_exact_height() {
        let mut db = CacheDB::<EmptyDB>::default();

        maybe_apply_eden_hardfork(&mut db, U256::from(100), Some(100)).expect("should succeed");

        let name = Database::storage(&mut db, WTIA_ADDRESS, NAME_SLOT)
            .expect("storage read should succeed");
        assert_eq!(name, NAME_VALUE);

        let symbol = Database::storage(&mut db, WTIA_ADDRESS, SYMBOL_SLOT)
            .expect("storage read should succeed");
        assert_eq!(symbol, SYMBOL_VALUE);

        let decimals = Database::storage(&mut db, WTIA_ADDRESS, DECIMALS_SLOT)
            .expect("storage read should succeed");
        assert_eq!(decimals, DECIMALS_VALUE);
    }

    #[test]
    fn hardfork_does_not_apply_at_wrong_height() {
        let mut db = CacheDB::<EmptyDB>::default();

        maybe_apply_eden_hardfork(&mut db, U256::from(99), Some(100)).expect("should succeed");

        let name = Database::storage(&mut db, WTIA_ADDRESS, NAME_SLOT)
            .expect("storage read should succeed");
        assert_eq!(name, U256::ZERO, "storage should be empty at height 99");

        maybe_apply_eden_hardfork(&mut db, U256::from(101), Some(100)).expect("should succeed");

        let name = Database::storage(&mut db, WTIA_ADDRESS, NAME_SLOT)
            .expect("storage read should succeed");
        assert_eq!(name, U256::ZERO, "storage should be empty at height 101");
    }

    #[test]
    fn hardfork_skipped_when_height_is_none() {
        let mut db = CacheDB::<EmptyDB>::default();

        maybe_apply_eden_hardfork(&mut db, U256::from(100), None).expect("should succeed");

        let name = Database::storage(&mut db, WTIA_ADDRESS, NAME_SLOT)
            .expect("storage read should succeed");
        assert_eq!(name, U256::ZERO, "storage should be empty when hardfork is disabled");
    }

    fn run_executor_at_height(hardfork_height: Option<u64>, parent_number: u64) -> bool {
        use std::sync::Arc;

        use alloy_evm::block::BlockExecutor as _;
        use ev_revm::with_ev_handler;
        use reth_chainspec::ChainSpecBuilder;
        use reth_evm::{execute::BlockBuilder as _, ConfigureEvm, NextBlockEnvAttributes};
        use reth_evm_ethereum::EthEvmConfig;
        use reth_revm::state::AccountInfo;

        let chain_spec = Arc::new(
            ChainSpecBuilder::default()
                .chain(reth_chainspec::Chain::from_id(1234))
                .genesis(Default::default())
                .cancun_activated()
                .build(),
        );

        let base_config = EthEvmConfig::new(chain_spec);
        let evolve_config = with_ev_handler(base_config, None, None, None, None);
        let eden_config = EdenEvmConfig::new(evolve_config, hardfork_height);

        let mut db = CacheDB::<EmptyDB>::default();
        db.insert_account_info(
            WTIA_ADDRESS,
            AccountInfo {
                nonce: 1,
                ..Default::default()
            },
        );
        let mut state = State::builder().with_database(db).with_bundle_update().build();

        let mut parent = alloy_consensus::Header::default();
        parent.number = parent_number;
        parent.timestamp = 1000;
        parent.gas_limit = 30_000_000;
        parent.base_fee_per_gas = Some(0);
        parent.excess_blob_gas = Some(0);
        parent.blob_gas_used = Some(0);
        parent.parent_beacon_block_root = Some(alloy_primitives::B256::ZERO);
        let sealed_parent =
            reth_primitives_traits::SealedHeader::new(parent, alloy_primitives::B256::ZERO);

        let attrs = NextBlockEnvAttributes {
            timestamp: 2000,
            suggested_fee_recipient: Address::ZERO,
            prev_randao: alloy_primitives::B256::ZERO,
            gas_limit: 30_000_000,
            parent_beacon_block_root: Some(alloy_primitives::B256::ZERO),
            withdrawals: Some(Default::default()),
        };

        {
            let mut builder = eden_config
                .builder_for_next_block(&mut state, &sealed_parent, attrs)
                .expect("builder creation should succeed");

            builder
                .apply_pre_execution_changes()
                .expect("pre-execution should succeed");

            let executor = builder.into_executor();
            let _ = executor.finish().expect("finish should succeed");
        }

        state.merge_transitions(revm::database::states::bundle_state::BundleRetention::Reverts);

        state
            .bundle_state
            .state
            .get(&WTIA_ADDRESS)
            .and_then(|acc| acc.storage.get(&NAME_SLOT))
            .map(|slot| slot.present_value == NAME_VALUE)
            .unwrap_or(false)
    }

    #[test]
    fn executor_injects_storage_at_hardfork_height() {
        assert!(
            run_executor_at_height(Some(5), 4),
            "WTIA storage should be injected at hardfork height"
        );
    }

    #[test]
    fn executor_skips_injection_at_other_heights() {
        assert!(
            !run_executor_at_height(Some(5), 3),
            "WTIA storage should NOT be injected one block early"
        );
        assert!(
            !run_executor_at_height(Some(5), 5),
            "WTIA storage should NOT be injected one block late"
        );
    }

    #[test]
    fn executor_skips_injection_when_disabled() {
        assert!(
            !run_executor_at_height(None, 4),
            "WTIA storage should NOT be injected when hardfork is disabled"
        );
    }
}
