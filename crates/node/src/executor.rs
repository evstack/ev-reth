//! Helpers to build the ev-reth executor with EV-specific hooks applied.

use alloy_consensus::{BlockHeader, Header};
use alloy_eips::Decodable2718;
use alloy_evm::eth::spec::EthExecutorSpec;
use alloy_evm::{FromRecoveredTx, FromTxWithEncoded};
use alloy_primitives::U256;
use alloy_rpc_types_engine::ExecutionData;
use ev_revm::{
    BaseFeeRedirect, BaseFeeRedirectSettings, ContractSizeLimitSettings, EvTxEvmFactory,
    MintPrecompileSettings,
};
use reth_chainspec::{ChainSpec, EthChainSpec};
use reth_ethereum::{
    chainspec::EthereumHardforks,
    node::{
        api::FullNodeTypes,
        builder::{components::ExecutorBuilder as RethExecutorBuilder, BuilderContext},
    },
};
use reth_ethereum_forks::Hardforks;
use reth_evm::{
    ConfigureEngineEvm, ConfigureEvm, EvmEnv, EvmEnvFor, ExecutableTxIterator, ExecutionCtxFor,
    NextBlockEnvAttributes, TransactionEnv,
};
use reth_node_builder::PayloadBuilderConfig;
use reth_primitives_traits::{
    constants::MAX_TX_GAS_LIMIT_OSAKA, SealedBlock, SealedHeader, SignedTransaction, TxTy,
};
use reth_revm::revm::{
    context::{BlockEnv, CfgEnv},
    context_interface::block::BlobExcessGasAndPrice,
    primitives::hardfork::SpecId,
};
use reth_errors::RethError;
use tracing::info;

use crate::evm_executor::{EvBlockExecutorFactory, EvReceiptBuilder};
use crate::{config::EvolvePayloadBuilderConfig, EvolveNode};
use ev_primitives::{EvPrimitives, EvTxEnvelope};
use reth_evm_ethereum::{revm_spec, revm_spec_by_timestamp_and_block_number, EthBlockAssembler};

/// Type alias for the EV-aware EVM config we install into the node.
pub type EvolveEvmConfig = EvEvmConfig<ChainSpec, EvTxEvmFactory>;

/// EVM config wired for EvPrimitives.
#[derive(Debug, Clone)]
pub struct EvEvmConfig<C = ChainSpec, EvmFactory = EvTxEvmFactory> {
    pub executor_factory: EvBlockExecutorFactory<EvReceiptBuilder, std::sync::Arc<C>, EvmFactory>,
    pub block_assembler: EthBlockAssembler<C>,
}

impl<ChainSpec> EvEvmConfig<ChainSpec> {
    pub fn new(chain_spec: std::sync::Arc<ChainSpec>) -> Self {
        Self::new_with_evm_factory(chain_spec, EvTxEvmFactory::default())
    }
}

impl<ChainSpec, EvmFactory> EvEvmConfig<ChainSpec, EvmFactory> {
    pub fn new_with_evm_factory(
        chain_spec: std::sync::Arc<ChainSpec>,
        evm_factory: EvmFactory,
    ) -> Self {
        Self {
            block_assembler: EthBlockAssembler::new(chain_spec.clone()),
            executor_factory: EvBlockExecutorFactory::new(EvReceiptBuilder, chain_spec, evm_factory),
        }
    }

    pub const fn chain_spec(&self) -> &std::sync::Arc<ChainSpec> {
        self.executor_factory.spec()
    }

    pub fn with_extra_data(mut self, extra_data: alloy_primitives::Bytes) -> Self {
        self.block_assembler.extra_data = extra_data;
        self
    }
}

impl<ChainSpec, EvmF> ConfigureEvm for EvEvmConfig<ChainSpec, EvmF>
where
    ChainSpec: EthExecutorSpec + EthChainSpec<Header = Header> + Hardforks + 'static,
    EvmF: reth_evm::EvmFactory<
            Tx: TransactionEnv,
            Spec = SpecId,
            Precompiles = reth_evm::precompiles::PrecompilesMap,
        > + Clone
        + std::fmt::Debug
        + Send
        + Sync
        + Unpin
        + 'static,
    EvmF::Tx: FromRecoveredTx<EvTxEnvelope> + FromTxWithEncoded<EvTxEnvelope> + Clone,
{
    type Primitives = EvPrimitives;
    type Error = std::convert::Infallible;
    type NextBlockEnvCtx = NextBlockEnvAttributes;
    type BlockExecutorFactory =
        EvBlockExecutorFactory<EvReceiptBuilder, std::sync::Arc<ChainSpec>, EvmF>;
    type BlockAssembler = EthBlockAssembler<ChainSpec>;

    fn block_executor_factory(&self) -> &Self::BlockExecutorFactory {
        &self.executor_factory
    }

    fn block_assembler(&self) -> &Self::BlockAssembler {
        &self.block_assembler
    }

    fn evm_env(&self, header: &Header) -> Result<EvmEnv, Self::Error> {
        let blob_params = self.chain_spec().blob_params_at_timestamp(header.timestamp);
        let spec = revm_spec(self.chain_spec(), header);

        let mut cfg_env =
            CfgEnv::new().with_chain_id(self.chain_spec().chain().id()).with_spec(spec);

        if let Some(blob_params) = &blob_params {
            cfg_env.set_max_blobs_per_tx(blob_params.max_blobs_per_tx);
        }

        if self.chain_spec().is_osaka_active_at_timestamp(header.timestamp) {
            cfg_env.tx_gas_limit_cap = Some(MAX_TX_GAS_LIMIT_OSAKA);
        }

        let blob_excess_gas_and_price =
            header.excess_blob_gas.zip(blob_params).map(|(excess_blob_gas, params)| {
                let blob_gasprice = params.calc_blob_fee(excess_blob_gas);
                BlobExcessGasAndPrice { excess_blob_gas, blob_gasprice }
            });

        let block_env = BlockEnv {
            number: U256::from(header.number),
            beneficiary: header.beneficiary,
            timestamp: U256::from(header.timestamp),
            difficulty: if spec >= SpecId::MERGE {
                U256::ZERO
            } else {
                header.difficulty
            },
            prevrandao: if spec >= SpecId::MERGE {
                Some(header.mix_hash)
            } else {
                None
            },
            gas_limit: header.gas_limit,
            basefee: header.base_fee_per_gas.unwrap_or_default(),
            blob_excess_gas_and_price,
        };

        Ok(EvmEnv { cfg_env, block_env })
    }

    fn next_evm_env(
        &self,
        parent: &Header,
        attributes: &NextBlockEnvAttributes,
    ) -> Result<EvmEnv, Self::Error> {
        let chain_spec = self.chain_spec();
        let blob_params = chain_spec.blob_params_at_timestamp(attributes.timestamp);
        let spec_id = revm_spec_by_timestamp_and_block_number(
            chain_spec,
            attributes.timestamp,
            parent.number() + 1,
        );

        let mut cfg =
            CfgEnv::new().with_chain_id(self.chain_spec().chain().id()).with_spec(spec_id);

        if let Some(blob_params) = &blob_params {
            cfg.set_max_blobs_per_tx(blob_params.max_blobs_per_tx);
        }

        if self.chain_spec().is_osaka_active_at_timestamp(attributes.timestamp) {
            cfg.tx_gas_limit_cap = Some(MAX_TX_GAS_LIMIT_OSAKA);
        }

        let blob_excess_gas_and_price =
            parent.excess_blob_gas.zip(blob_params).map(|(excess_blob_gas, params)| {
                let blob_gasprice = params.calc_blob_fee(excess_blob_gas);
                BlobExcessGasAndPrice { excess_blob_gas, blob_gasprice }
            });

        let mut gas_limit = parent.gas_limit;
        let mut basefee = None;

        if self
            .chain_spec()
            .fork(reth_ethereum_forks::EthereumHardfork::London)
            .transitions_at_block(parent.number + 1)
        {
            let elasticity_multiplier = self
                .chain_spec()
                .base_fee_params_at_timestamp(attributes.timestamp)
                .elasticity_multiplier;
            gas_limit *= elasticity_multiplier as u64;
            basefee = Some(alloy_eips::eip1559::INITIAL_BASE_FEE);
        }

        let block_env = BlockEnv {
            number: U256::from(parent.number + 1),
            beneficiary: attributes.suggested_fee_recipient,
            timestamp: U256::from(attributes.timestamp),
            difficulty: U256::ZERO,
            prevrandao: Some(attributes.prev_randao),
            gas_limit,
            basefee: basefee.unwrap_or_default(),
            blob_excess_gas_and_price,
        };

        Ok(EvmEnv { cfg_env: cfg, block_env })
    }

    fn context_for_block<'a>(
        &self,
        block: &'a SealedBlock<ev_primitives::Block>,
    ) -> Result<alloy_evm::eth::EthBlockExecutionCtx<'a>, Self::Error> {
        Ok(alloy_evm::eth::EthBlockExecutionCtx {
            parent_hash: block.header().parent_hash,
            parent_beacon_block_root: block.header().parent_beacon_block_root,
            ommers: &block.body().ommers,
            withdrawals: block.body().withdrawals.as_ref().map(std::borrow::Cow::Borrowed),
        })
    }

    fn context_for_next_block(
        &self,
        parent: &SealedHeader<Header>,
        attributes: Self::NextBlockEnvCtx,
    ) -> Result<alloy_evm::eth::EthBlockExecutionCtx<'_>, Self::Error> {
        Ok(alloy_evm::eth::EthBlockExecutionCtx {
            parent_hash: parent.hash(),
            parent_beacon_block_root: attributes.parent_beacon_block_root,
            ommers: &[],
            withdrawals: attributes.withdrawals.map(std::borrow::Cow::Owned),
        })
    }
}

impl<ChainSpec, EvmF> ConfigureEngineEvm<ExecutionData> for EvEvmConfig<ChainSpec, EvmF>
where
    ChainSpec: EthExecutorSpec + EthChainSpec<Header = Header> + Hardforks + 'static,
    EvmF: reth_evm::EvmFactory<
            Tx: TransactionEnv
                    + FromRecoveredTx<EvTxEnvelope>
                    + FromTxWithEncoded<EvTxEnvelope>,
            Spec = SpecId,
            Precompiles = reth_evm::precompiles::PrecompilesMap,
        > + Clone
        + std::fmt::Debug
        + Send
        + Sync
        + Unpin
        + 'static,
{
    fn evm_env_for_payload(&self, payload: &ExecutionData) -> EvmEnvFor<Self> {
        let timestamp = payload.payload.timestamp();
        let block_number = payload.payload.block_number();

        let blob_params = self.chain_spec().blob_params_at_timestamp(timestamp);
        let spec =
            revm_spec_by_timestamp_and_block_number(self.chain_spec(), timestamp, block_number);

        let mut cfg_env =
            CfgEnv::new().with_chain_id(self.chain_spec().chain().id()).with_spec(spec);

        if let Some(blob_params) = &blob_params {
            cfg_env.set_max_blobs_per_tx(blob_params.max_blobs_per_tx);
        }

        if self.chain_spec().is_osaka_active_at_timestamp(timestamp) {
            cfg_env.tx_gas_limit_cap = Some(MAX_TX_GAS_LIMIT_OSAKA);
        }

        let blob_excess_gas_and_price =
            payload.payload.excess_blob_gas().zip(blob_params).map(|(excess_blob_gas, params)| {
                let blob_gasprice = params.calc_blob_fee(excess_blob_gas);
                BlobExcessGasAndPrice { excess_blob_gas, blob_gasprice }
            });

        let block_env = BlockEnv {
            number: U256::from(block_number),
            beneficiary: payload.payload.fee_recipient(),
            timestamp: U256::from(timestamp),
            difficulty: if spec >= SpecId::MERGE {
                U256::ZERO
            } else {
                payload.payload.as_v1().prev_randao.into()
            },
            prevrandao: (spec >= SpecId::MERGE).then(|| payload.payload.as_v1().prev_randao),
            gas_limit: payload.payload.gas_limit(),
            basefee: payload.payload.saturated_base_fee_per_gas(),
            blob_excess_gas_and_price,
        };

        EvmEnv { cfg_env, block_env }
    }

    fn context_for_payload<'a>(&self, payload: &'a ExecutionData) -> ExecutionCtxFor<'a, Self> {
        alloy_evm::eth::EthBlockExecutionCtx {
            parent_hash: payload.parent_hash(),
            parent_beacon_block_root: payload.sidecar.parent_beacon_block_root(),
            ommers: &[],
            withdrawals: payload.payload.withdrawals().map(|w| std::borrow::Cow::Owned(w.clone().into())),
        }
    }

    fn tx_iterator_for_payload(&self, payload: &ExecutionData) -> impl ExecutableTxIterator<Self> {
        payload.payload.transactions().clone().into_iter().map(|tx| {
            let tx =
                TxTy::<Self::Primitives>::decode_2718_exact(tx.as_ref()).map_err(RethError::other)?;
            let signer = tx.try_recover().map_err(RethError::other)?;
            Ok::<_, RethError>(tx.with_signer(signer))
        })
    }
}

/// Builds the EV-aware EVM configuration by wrapping the default config with the EV handler.
pub fn build_evm_config<Node>(ctx: &BuilderContext<Node>) -> eyre::Result<EvolveEvmConfig>
where
    Node: FullNodeTypes<Types = EvolveNode>,
    ChainSpec: Hardforks + EthExecutorSpec + EthereumHardforks,
{
    let chain_spec = ctx.chain_spec();

    let evolve_config = EvolvePayloadBuilderConfig::from_chain_spec(chain_spec.as_ref())?;
    evolve_config.validate()?;

    let redirect = evolve_config
        .base_fee_redirect_settings()
        .map(|(sink, activation)| {
            info!(
                target = "ev-reth::executor",
                fee_sink = ?sink,
                activation_height = activation,
                "Base fee redirect enabled"
            );
            BaseFeeRedirectSettings::new(BaseFeeRedirect::new(sink), activation)
        });

    let mint_precompile = evolve_config
        .mint_precompile_settings()
        .map(|(admin, activation)| MintPrecompileSettings::new(admin, activation));

    let contract_size_limit =
        evolve_config
            .contract_size_limit_settings()
            .map(|(limit, activation)| {
                info!(
                    target = "ev-reth::executor",
                    limit_bytes = limit,
                    activation_height = activation,
                    "Custom contract size limit enabled"
                );
                ContractSizeLimitSettings::new(limit, activation)
            });

    let factory = EvTxEvmFactory::new(
        redirect,
        mint_precompile,
        contract_size_limit,
    );

    Ok(EvEvmConfig::new_with_evm_factory(chain_spec.clone(), factory)
        .with_extra_data(ctx.payload_builder_config().extra_data_bytes()))
}

/// Thin wrapper so we can plug the EV executor into the node components builder.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct EvolveExecutorBuilder;

impl<Node> RethExecutorBuilder<Node> for EvolveExecutorBuilder
where
    Node: FullNodeTypes<Types = EvolveNode>,
    ChainSpec: Hardforks + EthExecutorSpec + EthereumHardforks,
{
    type EVM = EvolveEvmConfig;

    async fn build_evm(self, ctx: &BuilderContext<Node>) -> eyre::Result<Self::EVM> {
        build_evm_config(ctx)
    }
}
