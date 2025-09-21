use reth_ethereum::evm::{
    primitives::{eth::EthEvmContext, precompiles::PrecompilesMap, Database, EvmEnv, EvmFactory},
    revm::{
        context::{
            result::{EVMError, HaltReason},
            TxEnv,
        },
        handler::EthPrecompiles,
        inspector::NoOpInspector,
        interpreter::interpreter::EthInterpreter,
        primitives::hardfork::SpecId,
        Context, Inspector, MainBuilder, MainContext,
    },
    EthEvm,
};

use crate::precompiles::custom_prague_precompiles;

/// Rollkit EVM factory that allows custom precompiles.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct RollkitEvmFactory;

impl EvmFactory for RollkitEvmFactory {
    type Evm<DB: Database, I: Inspector<EthEvmContext<DB>, EthInterpreter>> =
        EthEvm<DB, I, Self::Precompiles>;
    type Context<DB: Database> = EthEvmContext<DB>;
    type Tx = TxEnv;
    type Error<DBError: core::error::Error + Send + Sync + 'static> = EVMError<DBError>;
    type HaltReason = HaltReason;
    type Spec = SpecId;
    type Precompiles = PrecompilesMap;

    fn create_evm<DB: Database>(&self, db: DB, input: EvmEnv) -> Self::Evm<DB, NoOpInspector> {
        let spec = input.cfg_env.spec;
        let mut evm = Context::mainnet()
            .with_db(db)
            .with_cfg(input.cfg_env)
            .with_block(input.block_env)
            .build_mainnet_with_inspector(NoOpInspector {})
            .with_precompiles(PrecompilesMap::from_static(
                EthPrecompiles::default().precompiles,
            ));

        if spec == SpecId::PRAGUE {
            evm = evm.with_precompiles(PrecompilesMap::from_static(custom_prague_precompiles()));
        }

        EthEvm::new(evm, false)
    }

    fn create_evm_with_inspector<DB: Database, I: Inspector<Self::Context<DB>, EthInterpreter>>(
        &self,
        db: DB,
        input: EvmEnv,
        inspector: I,
    ) -> Self::Evm<DB, I> {
        EthEvm::new(
            self.create_evm(db, input)
                .into_inner()
                .with_inspector(inspector),
            true,
        )
    }
}
