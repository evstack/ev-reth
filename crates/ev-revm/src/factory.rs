//! Helpers for wrapping Reth EVM factories with the EV handler.

use crate::{base_fee::BaseFeeRedirect, evm::EvEvm};
use alloy_evm::{
    eth::{EthBlockExecutorFactory, EthEvmContext, EthEvmFactory},
    precompiles::{DynPrecompile, Precompile, PrecompilesMap},
    Database, EvmEnv, EvmFactory,
};
use alloy_primitives::Address;
use ev_precompiles::mint::{MintPrecompile, MINT_PRECOMPILE_ADDR};
use reth_evm_ethereum::EthEvmConfig;
use reth_revm::{
    inspector::NoOpInspector,
    revm::{
        context::{
            result::{EVMError, HaltReason},
            BlockEnv, TxEnv,
        },
        context_interface::result::InvalidTransaction,
        primitives::hardfork::SpecId,
        Inspector,
    },
};
use std::sync::Arc;

/// Wrapper around an existing `EvmFactory` that produces [`EvEvm`] instances.
#[derive(Debug, Clone)]
pub struct EvEvmFactory<F> {
    inner: F,
    redirect: Option<BaseFeeRedirect>,
    mint_admin: Option<Address>,
}

impl<F> EvEvmFactory<F> {
    /// Creates a new factory wrapper with the given redirect policy.
    pub const fn new(
        inner: F,
        redirect: Option<BaseFeeRedirect>,
        mint_admin: Option<Address>,
    ) -> Self {
        Self {
            inner,
            redirect,
            mint_admin,
        }
    }

    fn install_mint_precompile(&self, precompiles: &mut PrecompilesMap) {
        let Some(admin) = self.mint_admin else { return };

        let mint = Arc::new(MintPrecompile::new(admin));
        let id = MintPrecompile::id().clone();

        precompiles.apply_precompile(&MINT_PRECOMPILE_ADDR, move |_| {
            let mint_for_call = Arc::clone(&mint);
            let id_for_call = id;
            Some(DynPrecompile::new_stateful(id_for_call, move |input| {
                mint_for_call.call(input)
            }))
        });
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
    type BlockEnv = BlockEnv;
    type Precompiles = PrecompilesMap;

    fn create_evm<DB: Database>(
        &self,
        db: DB,
        evm_env: EvmEnv<Self::Spec, Self::BlockEnv>,
    ) -> Self::Evm<DB, NoOpInspector> {
        let inner = self.inner.create_evm(db, evm_env);
        let mut evm = EvEvm::from_inner(inner, self.redirect, false);
        {
            let inner = evm.inner_mut();
            self.install_mint_precompile(&mut inner.precompiles);
        }
        evm
    }

    fn create_evm_with_inspector<DB: Database, I: Inspector<Self::Context<DB>>>(
        &self,
        db: DB,
        input: EvmEnv<Self::Spec, Self::BlockEnv>,
        inspector: I,
    ) -> Self::Evm<DB, I> {
        let inner = self.inner.create_evm_with_inspector(db, input, inspector);
        let mut evm = EvEvm::from_inner(inner, self.redirect, true);
        {
            let inner = evm.inner_mut();
            self.install_mint_precompile(&mut inner.precompiles);
        }
        evm
    }
}

/// Wraps an [`EthEvmConfig`] so that it produces [`EvEvm`] instances.
pub fn with_ev_handler<ChainSpec>(
    config: EthEvmConfig<ChainSpec, EthEvmFactory>,
    redirect: Option<BaseFeeRedirect>,
    mint_admin: Option<Address>,
) -> EthEvmConfig<ChainSpec, EvEvmFactory<EthEvmFactory>> {
    let EthEvmConfig {
        executor_factory,
        block_assembler,
    } = config;
    let wrapped_factory = EvEvmFactory::new(*executor_factory.evm_factory(), redirect, mint_admin);
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
    use alloy_primitives::{address, keccak256, Address, Bytes, TxKind, U256};
    use alloy_sol_types::{sol, SolCall};
    use reth_revm::{
        revm::{
            bytecode::Bytecode as RevmBytecode,
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
        let burn_address = Address::ZERO;

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

        let mut evm_env: alloy_evm::EvmEnv<SpecId, BlockEnv> = EvmEnv::default();
        evm_env.cfg_env.chain_id = 1;
        evm_env.cfg_env.spec = SpecId::CANCUN;
        evm_env.block_env.basefee = 100;
        evm_env.block_env.number = U256::from(1);
        evm_env.block_env.beneficiary = beneficiary;
        evm_env.block_env.gas_limit = 30_000_000;

        let redirect = BaseFeeRedirect::new(sink);
        let mut evm = EvEvmFactory::new(
            alloy_evm::eth::EthEvmFactory::default(),
            Some(redirect),
            None,
        )
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

        let burn_balance_intact = state
            .get(&burn_address)
            .map(|account| account.info.balance.is_zero())
            .unwrap_or(true);
        assert!(burn_balance_intact, "burn address balance must remain zero");
    }

    #[test]
    fn mint_precompile_via_proxy_runtime_mints() {
        sol! {
            contract MintAdminProxy {
                function mint(address to, uint256 amount);
            }
        }

        const ADMIN_PROXY_RUNTIME: [u8; 42] = alloy_primitives::hex!(
            "36600060003760006000366000600073000000000000000000000000000000000000f1005af1600080f3"
        );

        let caller = address!("0x0000000000000000000000000000000000000aaa");
        let contract = address!("0x0000000000000000000000000000000000000bbb");
        let mintee = address!("0x0000000000000000000000000000000000000ccc");
        let amount = U256::from(1_000_000_000_000_000u64);

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
            contract,
            AccountInfo {
                balance: U256::ZERO,
                nonce: 1,
                code_hash: keccak256(ADMIN_PROXY_RUNTIME.as_slice()),
                code: Some(RevmBytecode::new_raw(Bytes::copy_from_slice(
                    ADMIN_PROXY_RUNTIME.as_slice(),
                ))),
            },
        );

        let mut evm_env: alloy_evm::EvmEnv<SpecId, BlockEnv> = EvmEnv::default();
        evm_env.cfg_env.chain_id = 1;
        evm_env.cfg_env.spec = SpecId::CANCUN;
        evm_env.block_env.gas_limit = 30_000_000;
        evm_env.block_env.number = U256::from(1);
        evm_env.block_env.basefee = 1;

        let mut evm = EvEvmFactory::new(
            alloy_evm::eth::EthEvmFactory::default(),
            None,
            Some(contract),
        )
        .create_evm(state, evm_env);

        let tx = crate::factory::TxEnv {
            caller,
            kind: TxKind::Call(contract),
            gas_limit: 500_000,
            gas_price: 1,
            value: U256::ZERO,
            data: MintAdminProxy::mintCall { to: mintee, amount }
                .abi_encode()
                .into(),
            ..Default::default()
        };

        let result_and_state = evm
            .transact_raw(tx)
            .expect("proxy call executes without error");

        let ExecutionResult::Success { .. } = result_and_state.result else {
            panic!("expected successful mint execution via proxy")
        };

        let state: EvmState = result_and_state.state;
        let mintee_account = state
            .get(&mintee)
            .expect("mint precompile should create mintee account");
        assert_eq!(
            mintee_account.info.balance, amount,
            "mint proxy should credit the recipient"
        );
    }
}
