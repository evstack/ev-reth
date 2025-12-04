//! Helpers for wrapping Reth EVM factories with the EV handler.

use crate::{base_fee::BaseFeeRedirect, evm::EvEvm};
use alloy_evm::{
    eth::{EthBlockExecutorFactory, EthEvmContext, EthEvmFactory},
    precompiles::{DynPrecompile, Precompile, PrecompilesMap},
    Database, EvmEnv, EvmFactory,
};
use alloy_primitives::{Address, U256};
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

/// Settings for enabling the base-fee redirect at a specific block height.
#[derive(Debug, Clone, Copy)]
pub struct BaseFeeRedirectSettings {
    redirect: BaseFeeRedirect,
    activation_height: u64,
}

impl BaseFeeRedirectSettings {
    /// Creates a new settings object.
    pub const fn new(redirect: BaseFeeRedirect, activation_height: u64) -> Self {
        Self {
            redirect,
            activation_height,
        }
    }

    const fn activation_height(&self) -> u64 {
        self.activation_height
    }

    const fn redirect(&self) -> BaseFeeRedirect {
        self.redirect
    }
}

/// Settings for enabling the mint precompile at a specific block height.
#[derive(Debug, Clone, Copy)]
pub struct MintPrecompileSettings {
    admin: Address,
    activation_height: u64,
}

impl MintPrecompileSettings {
    /// Creates a new settings object.
    pub const fn new(admin: Address, activation_height: u64) -> Self {
        Self {
            admin,
            activation_height,
        }
    }

    const fn activation_height(&self) -> u64 {
        self.activation_height
    }

    const fn admin(&self) -> Address {
        self.admin
    }
}

/// Settings for custom contract size limit with activation height.
#[derive(Debug, Clone, Copy)]
pub struct ContractSizeLimitSettings {
    limit: usize,
    activation_height: u64,
}

impl ContractSizeLimitSettings {
    /// Creates a new settings object.
    pub const fn new(limit: usize, activation_height: u64) -> Self {
        Self {
            limit,
            activation_height,
        }
    }

    const fn activation_height(&self) -> u64 {
        self.activation_height
    }

    const fn limit(&self) -> usize {
        self.limit
    }
}

/// Wrapper around an existing `EvmFactory` that produces [`EvEvm`] instances.
#[derive(Debug, Clone)]
pub struct EvEvmFactory<F> {
    inner: F,
    redirect: Option<BaseFeeRedirectSettings>,
    mint_precompile: Option<MintPrecompileSettings>,
    contract_size_limit: Option<ContractSizeLimitSettings>,
}

impl<F> EvEvmFactory<F> {
    /// Creates a new factory wrapper with the given redirect policy.
    pub const fn new(
        inner: F,
        redirect: Option<BaseFeeRedirectSettings>,
        mint_precompile: Option<MintPrecompileSettings>,
        contract_size_limit: Option<ContractSizeLimitSettings>,
    ) -> Self {
        Self {
            inner,
            redirect,
            mint_precompile,
            contract_size_limit,
        }
    }

    fn contract_size_limit_for_block(&self, block_number: U256) -> Option<usize> {
        self.contract_size_limit.and_then(|settings| {
            if block_number >= U256::from(settings.activation_height()) {
                Some(settings.limit())
            } else {
                None
            }
        })
    }

    fn install_mint_precompile(&self, precompiles: &mut PrecompilesMap, block_number: U256) {
        let Some(settings) = self.mint_precompile else {
            return;
        };
        if block_number < U256::from(settings.activation_height()) {
            return;
        }

        let mint = Arc::new(MintPrecompile::new(settings.admin()));
        let id = MintPrecompile::id().clone();

        precompiles.apply_precompile(&MINT_PRECOMPILE_ADDR, move |_| {
            let mint_for_call = Arc::clone(&mint);
            let id_for_call = id;
            Some(DynPrecompile::new_stateful(id_for_call, move |input| {
                mint_for_call.call(input)
            }))
        });
    }

    fn redirect_for_block(&self, block_number: U256) -> Option<BaseFeeRedirect> {
        self.redirect.and_then(|settings| {
            if block_number >= U256::from(settings.activation_height()) {
                Some(settings.redirect())
            } else {
                None
            }
        })
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
        mut evm_env: EvmEnv<Self::Spec, Self::BlockEnv>,
    ) -> Self::Evm<DB, NoOpInspector> {
        let block_number = evm_env.block_env.number;
        // Apply custom contract size limit if configured and active for this block
        if let Some(limit) = self.contract_size_limit_for_block(block_number) {
            evm_env.cfg_env.limit_contract_code_size = Some(limit);
        }
        let inner = self.inner.create_evm(db, evm_env);
        let mut evm = EvEvm::from_inner(inner, self.redirect_for_block(block_number), false);
        {
            let inner = evm.inner_mut();
            self.install_mint_precompile(&mut inner.precompiles, block_number);
        }
        evm
    }

    fn create_evm_with_inspector<DB: Database, I: Inspector<Self::Context<DB>>>(
        &self,
        db: DB,
        mut input: EvmEnv<Self::Spec, Self::BlockEnv>,
        inspector: I,
    ) -> Self::Evm<DB, I> {
        let block_number = input.block_env.number;
        // Apply custom contract size limit if configured and active for this block
        if let Some(limit) = self.contract_size_limit_for_block(block_number) {
            input.cfg_env.limit_contract_code_size = Some(limit);
        }
        let inner = self.inner.create_evm_with_inspector(db, input, inspector);
        let mut evm = EvEvm::from_inner(inner, self.redirect_for_block(block_number), true);
        {
            let inner = evm.inner_mut();
            self.install_mint_precompile(&mut inner.precompiles, block_number);
        }
        evm
    }
}

/// Wraps an [`EthEvmConfig`] so that it produces [`EvEvm`] instances.
pub fn with_ev_handler<ChainSpec>(
    config: EthEvmConfig<ChainSpec, EthEvmFactory>,
    redirect: Option<BaseFeeRedirectSettings>,
    mint_precompile: Option<MintPrecompileSettings>,
    contract_size_limit: Option<ContractSizeLimitSettings>,
) -> EthEvmConfig<ChainSpec, EvEvmFactory<EthEvmFactory>> {
    let EthEvmConfig {
        executor_factory,
        block_assembler,
    } = config;
    let wrapped_factory = EvEvmFactory::new(
        *executor_factory.evm_factory(),
        redirect,
        mint_precompile,
        contract_size_limit,
    );
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

    sol! {
        contract MintAdminProxy {
            function mint(address to, uint256 amount);
        }
    }

    const ADMIN_PROXY_RUNTIME: [u8; 42] = alloy_primitives::hex!(
        "36600060003760006000366000600073000000000000000000000000000000000000f1005af1600080f3"
    );

    fn empty_state() -> State<CacheDB<EmptyDB>> {
        State::builder()
            .with_database(CacheDB::<EmptyDB>::default())
            .with_bundle_update()
            .build()
    }

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
            Some(BaseFeeRedirectSettings::new(redirect, 0)),
            None,
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
            Some(MintPrecompileSettings::new(contract, 0)),
            None,
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

    #[test]
    fn base_fee_redirect_respects_activation_height() {
        let sink = address!("0x0000000000000000000000000000000000000123");
        let factory = EvEvmFactory::new(
            alloy_evm::eth::EthEvmFactory::default(),
            Some(BaseFeeRedirectSettings::new(BaseFeeRedirect::new(sink), 5)),
            None,
            None,
        );

        let mut before_env: alloy_evm::EvmEnv<SpecId, BlockEnv> = EvmEnv::default();
        before_env.cfg_env.chain_id = 1;
        before_env.cfg_env.spec = SpecId::CANCUN;
        before_env.block_env.number = U256::from(4);
        before_env.block_env.gas_limit = 30_000_000;

        let mut after_env = before_env.clone();
        after_env.block_env.number = U256::from(5);

        let evm_before = factory.create_evm(empty_state(), before_env);
        assert!(
            evm_before.redirect().is_none(),
            "redirect inactive before fork"
        );

        let evm_after = factory.create_evm(empty_state(), after_env);
        assert!(
            evm_after.redirect().is_some(),
            "redirect active at fork height"
        );
    }

    #[test]
    fn mint_precompile_respects_activation_height() {
        let caller = address!("0x0000000000000000000000000000000000000aaa");
        let contract = address!("0x0000000000000000000000000000000000000bbb");
        let mintee = address!("0x0000000000000000000000000000000000000ccc");
        let amount = U256::from(1_000_000u64);

        let build_state = || {
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

            state
        };

        let factory = EvEvmFactory::new(
            alloy_evm::eth::EthEvmFactory::default(),
            None,
            Some(MintPrecompileSettings::new(contract, 3)),
            None,
        );

        let tx_env = || crate::factory::TxEnv {
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

        let mut before_env: alloy_evm::EvmEnv<SpecId, BlockEnv> = EvmEnv::default();
        before_env.cfg_env.chain_id = 1;
        before_env.cfg_env.spec = SpecId::CANCUN;
        before_env.block_env.number = U256::from(2);
        before_env.block_env.basefee = 1;
        before_env.block_env.gas_limit = 30_000_000;

        let mut evm_before = factory.create_evm(build_state(), before_env);
        let result_before = evm_before
            .transact_raw(tx_env())
            .expect("pre-activation call executes");
        let state: EvmState = result_before.state;
        assert!(
            !state.contains_key(&mintee),
            "precompile must not mint before activation height"
        );

        let mut after_env: alloy_evm::EvmEnv<SpecId, BlockEnv> = EvmEnv::default();
        after_env.cfg_env.chain_id = 1;
        after_env.cfg_env.spec = SpecId::CANCUN;
        after_env.block_env.number = U256::from(3);
        after_env.block_env.basefee = 1;
        after_env.block_env.gas_limit = 30_000_000;

        let mut evm_after = factory.create_evm(build_state(), after_env);
        let result_after = evm_after
            .transact_raw(tx_env())
            .expect("post-activation call executes");
        let state: EvmState = result_after.state;
        let mintee_account = state
            .get(&mintee)
            .expect("mint precompile should mint after activation");
        assert_eq!(mintee_account.info.balance, amount);
    }
}
