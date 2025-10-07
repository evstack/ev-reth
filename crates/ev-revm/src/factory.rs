//! Helpers for wrapping Reth EVM factories with the EV handler.

use crate::{base_fee::BaseFeeRedirect, evm::EvEvm};
use alloy_evm::{
    eth::{EthBlockExecutorFactory, EthEvmContext, EthEvmFactory},
    precompiles::PrecompilesMap,
    Database, EvmEnv, EvmFactory,
};
use reth_evm_ethereum::EthEvmConfig;
use reth_revm::{
    inspector::NoOpInspector,
    revm::{
        context::{
            result::{EVMError, HaltReason},
            TxEnv,
        },
        context_interface::result::InvalidTransaction,
        primitives::hardfork::SpecId,
        Inspector,
    },
};

/// Wrapper around an existing `EvmFactory` that produces [`EvEvm`] instances.
#[derive(Debug, Clone)]
pub struct EvEvmFactory<F> {
    inner: F,
    redirect: Option<BaseFeeRedirect>,
}

impl<F> EvEvmFactory<F> {
    /// Creates a new factory wrapper with the given redirect policy.
    pub const fn new(inner: F, redirect: Option<BaseFeeRedirect>) -> Self {
        Self { inner, redirect }
    }
}

impl EvmFactory for EvEvmFactory<EthEvmFactory> {
    type Evm<DB: Database, I: Inspector<Self::Context<DB>>> =
        EvEvm<EthEvmContext<DB>, I, PrecompilesMap>;
    type Context<DB: Database> = EthEvmContext<DB>;
    type Tx = TxEnv;
    type Error<DBError: std::error::Error + Send + Sync + 'static> =
        EVMError<DBError, InvalidTransaction>;
    type HaltReason = HaltReason;
    type Spec = SpecId;
    type Precompiles = PrecompilesMap;

    fn create_evm<DB: Database>(
        &self,
        db: DB,
        evm_env: EvmEnv<Self::Spec>,
    ) -> Self::Evm<DB, NoOpInspector> {
        let inner = self.inner.create_evm(db, evm_env);
        EvEvm::from_inner(inner, self.redirect, false)
    }

    fn create_evm_with_inspector<DB: Database, I: Inspector<Self::Context<DB>>>(
        &self,
        db: DB,
        input: EvmEnv<Self::Spec>,
        inspector: I,
    ) -> Self::Evm<DB, I> {
        let inner = self.inner.create_evm_with_inspector(db, input, inspector);
        EvEvm::from_inner(inner, self.redirect, true)
    }
}

/// Wraps an [`EthEvmConfig`] so that it produces [`EvEvm`] instances.
pub fn with_ev_handler<ChainSpec>(
    config: EthEvmConfig<ChainSpec, EthEvmFactory>,
    redirect: Option<BaseFeeRedirect>,
) -> EthEvmConfig<ChainSpec, EvEvmFactory<EthEvmFactory>> {
    let EthEvmConfig {
        executor_factory,
        block_assembler,
    } = config;
    let wrapped_factory = EvEvmFactory::new(*executor_factory.evm_factory(), redirect);
    let new_executor_factory = EthBlockExecutorFactory::new(
        *executor_factory.receipt_builder(),
        executor_factory.spec().clone(),
        wrapped_factory,
    );

    EthEvmConfig {
        executor_factory: new_executor_factory,
        block_assembler,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::SpecId;
    use alloy_evm::{Evm, EvmEnv};
    use alloy_primitives::{address, Bytes, TxKind, U256};
    use reth_revm::{
        revm::{
            context_interface::result::ExecutionResult,
            database::{CacheDB, EmptyDB},
            primitives::KECCAK_EMPTY,
            state::{AccountInfo, EvmState},
        },
        State,
    };

    #[test]
    fn factory_applies_base_fee_redirect() {
        let sink = address!("0x00000000000000000000000000000000000000fe");
        let beneficiary = address!("0x00000000000000000000000000000000000000be");
        let caller = address!("0x00000000000000000000000000000000000000ca");
        let target = address!("0x00000000000000000000000000000000000000aa");

        let mut state = State::builder()
            .with_database(CacheDB::<EmptyDB>::default())
            .with_bundle_update()
            .build();

        state.insert_account(
            caller,
            AccountInfo {
                balance: U256::from(1_000_000_000u64),
                nonce: 0,
                code_hash: KECCAK_EMPTY,
                code: None,
            },
        );

        let mut evm_env = EvmEnv::default();
        evm_env.cfg_env.chain_id = 1;
        evm_env.cfg_env.spec = SpecId::CANCUN;
        evm_env.block_env.basefee = 100;
        evm_env.block_env.number = U256::from(1);
        evm_env.block_env.beneficiary = beneficiary;
        evm_env.block_env.gas_limit = 30_000_000;

        let redirect = BaseFeeRedirect::new(sink);
        let mut evm = EvEvmFactory::new(alloy_evm::eth::EthEvmFactory::default(), Some(redirect))
            .create_evm(state, evm_env.clone());

        let tx = crate::factory::TxEnv {
            caller,
            kind: TxKind::Call(target),
            gas_limit: 21_000,
            gas_price: 200,
            value: U256::ZERO,
            data: Bytes::new(),
            ..Default::default()
        };

        // Save gas_price before tx is moved
        let gas_price = tx.gas_price;

        let result_and_state = evm
            .transact_raw(tx)
            .expect("transaction executes without error");

        let ExecutionResult::Success { gas_used, .. } = result_and_state.result else {
            panic!("expected successful execution");
        };

        let state: EvmState = result_and_state.state;
        let sink_account = state
            .get(&sink)
            .expect("base-fee sink credited during execution");
        let expected_redirect = U256::from(evm_env.block_env.basefee) * U256::from(gas_used);
        assert_eq!(sink_account.info.balance, expected_redirect);

        let tip_per_gas = U256::from(gas_price - evm_env.block_env.basefee as u128);
        let expected_tip = tip_per_gas * U256::from(gas_used);
        let beneficiary_account = state
            .get(&beneficiary)
            .expect("beneficiary receives priority fee");
        assert_eq!(beneficiary_account.info.balance, expected_tip);
    }
}
