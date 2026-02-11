use std::sync::Arc;

use alloy_primitives::{Address, U256};
use evolve_ev_reth::EvolvePayloadAttributes;
use eyre::WrapErr;
use reth_basic_payload_builder::{
    BuildArguments, BuildOutcome, HeaderForPayload, MissingPayloadBehaviour, PayloadBuilder,
    PayloadConfig,
};
use reth_ethereum::{
    chainspec::{ChainSpec, ChainSpecProvider},
    node::{
        api::{payload::PayloadBuilderAttributes, FullNodeTypes, NodeTypes},
        builder::{components::PayloadBuilderBuilder, BuilderContext},
    },
    pool::{PoolTransaction, TransactionPool},
    primitives::Header,
};
use reth_payload_builder::PayloadBuilderError;
use reth_provider::HeaderProvider;
use reth_revm::cached::CachedReads;
use tokio::runtime::Handle;
use tracing::info;

use crate::{
    attributes::EvolveEnginePayloadBuilderAttributes, builder::EvolvePayloadBuilder,
    config::EvolvePayloadBuilderConfig, executor::EvolveEvmConfig, node::EvolveEngineTypes,
    payload_types::EvBuiltPayload,
};

use ev_primitives::{EvPrimitives, TransactionSigned};
use evolve_ev_reth::config::set_current_block_gas_limit;

/// Evolve payload service builder that integrates with the evolve payload builder.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct EvolvePayloadBuilderBuilder {
    config: EvolvePayloadBuilderConfig,
}

impl EvolvePayloadBuilderBuilder {
    /// Create a new builder with evolve args.
    pub fn new() -> Self {
        let config = EvolvePayloadBuilderConfig::new();
        info!("Created Evolve payload builder with config: {:?}", config);
        Self { config }
    }
}

impl Default for EvolvePayloadBuilderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// The evolve engine payload builder that integrates with the evolve payload builder.
#[derive(Debug, Clone)]
pub struct EvolveEnginePayloadBuilder<Client>
where
    Client: Clone,
{
    pub(crate) evolve_builder: Arc<EvolvePayloadBuilder<Client>>,
    pub(crate) config: EvolvePayloadBuilderConfig,
}

impl<Node, Pool> PayloadBuilderBuilder<Node, Pool, EvolveEvmConfig> for EvolvePayloadBuilderBuilder
where
    Node: FullNodeTypes<
        Types: NodeTypes<
            Payload = EvolveEngineTypes,
            ChainSpec = ChainSpec,
            Primitives = EvPrimitives,
        >,
    >,
    Pool: TransactionPool<Transaction: PoolTransaction<Consensus = TransactionSigned>>
        + Unpin
        + 'static,
{
    type PayloadBuilder = EvolveEnginePayloadBuilder<Node::Provider>;

    async fn build_payload_builder(
        self,
        ctx: &BuilderContext<Node>,
        _pool: Pool,
        evm_config: EvolveEvmConfig,
    ) -> eyre::Result<Self::PayloadBuilder> {
        let chain_spec = ctx.chain_spec();
        let mut config = EvolvePayloadBuilderConfig::from_chain_spec(&chain_spec)
            .wrap_err("failed to load evolve config from chain spec")?;

        if self.config.base_fee_sink.is_some() {
            config.base_fee_sink = self.config.base_fee_sink;
        }
        if self.config.base_fee_redirect_activation_height.is_some() {
            config.base_fee_redirect_activation_height =
                self.config.base_fee_redirect_activation_height;
        }
        if self.config.mint_admin.is_some() {
            config.mint_admin = self.config.mint_admin;
        }
        if self.config.mint_precompile_activation_height.is_some() {
            config.mint_precompile_activation_height =
                self.config.mint_precompile_activation_height;
        }

        config.validate()?;

        let evolve_builder = Arc::new(EvolvePayloadBuilder::new(
            Arc::new(ctx.provider().clone()),
            evm_config,
            config.clone(),
        ));

        Ok(EvolveEnginePayloadBuilder {
            evolve_builder,
            config,
        })
    }
}

impl<Client> PayloadBuilder for EvolveEnginePayloadBuilder<Client>
where
    Client: reth_ethereum::provider::StateProviderFactory
        + ChainSpecProvider<ChainSpec = ChainSpec>
        + HeaderProvider<Header = Header>
        + Clone
        + Send
        + Sync
        + 'static,
{
    type Attributes = EvolveEnginePayloadBuilderAttributes;
    type BuiltPayload = EvBuiltPayload;

    fn try_build(
        &self,
        args: BuildArguments<Self::Attributes, Self::BuiltPayload>,
    ) -> Result<BuildOutcome<Self::BuiltPayload>, PayloadBuilderError> {
        let BuildArguments {
            cached_reads: _,
            config,
            cancel: _,
            best_payload: _,
        } = args;
        let PayloadConfig {
            parent_header,
            attributes,
        } = config;

        info!(
            "Evolve engine payload builder: building payload with {} transactions",
            attributes.transactions.len()
        );

        // Convert Engine API attributes to Evolve payload attributes.
        // If no gas_limit provided, default to the parent header's gas limit (genesis for first block).
        let effective_gas_limit = attributes.gas_limit.unwrap_or(parent_header.gas_limit);
        // Publish effective gas limit for RPC alignment.
        set_current_block_gas_limit(effective_gas_limit);

        let block_number = parent_header.number + 1;
        let mut fee_recipient = attributes.suggested_fee_recipient();
        if fee_recipient == Address::ZERO {
            if let Some(sink) = self.config.base_fee_sink_for_block(block_number) {
                info!(
                    target: "ev-reth",
                    fee_sink = ?sink,
                    block_number,
                    "Suggested fee recipient missing; defaulting to base-fee sink"
                );
                fee_recipient = sink;
            }
        }

        let evolve_attrs = EvolvePayloadAttributes::new(
            attributes.transactions.clone(),
            Some(effective_gas_limit),
            attributes.timestamp(),
            attributes.prev_randao(),
            fee_recipient,
            attributes.parent(),
            block_number,
        );

        // Build the payload using the evolve payload builder - use spawn_blocking for async work.
        let evolve_builder = self.evolve_builder.clone();
        let sealed_block = tokio::task::block_in_place(|| {
            Handle::current().block_on(evolve_builder.build_payload(evolve_attrs))
        })
        .map_err(PayloadBuilderError::other)?;

        info!(
            "Evolve engine payload builder: built block with {} transactions, gas used: {}",
            sealed_block.transaction_count(),
            sealed_block.gas_used
        );

        // Convert to EvBuiltPayload.
        let gas_used = sealed_block.gas_used;
        let built_payload = EvBuiltPayload::new(
            attributes.payload_id(), // Use the proper payload ID from attributes.
            Arc::new(sealed_block),
            U256::from(gas_used), // Block gas used.
            None,                 // No blob sidecar for evolve.
        );

        Ok(BuildOutcome::Better {
            payload: built_payload,
            cached_reads: CachedReads::default(),
        })
    }

    fn build_empty_payload(
        &self,
        config: PayloadConfig<Self::Attributes, HeaderForPayload<Self::BuiltPayload>>,
    ) -> Result<Self::BuiltPayload, PayloadBuilderError> {
        let PayloadConfig {
            parent_header,
            attributes,
        } = config;

        info!("Evolve engine payload builder: building empty payload");

        // Create empty evolve attributes (no transactions).
        // If no gas_limit provided, default to the parent header's gas limit (genesis for first block).
        let effective_gas_limit = attributes.gas_limit.unwrap_or(parent_header.gas_limit);
        // Publish effective gas limit for RPC alignment.
        set_current_block_gas_limit(effective_gas_limit);

        let block_number = parent_header.number + 1;
        let mut fee_recipient = attributes.suggested_fee_recipient();
        if fee_recipient == Address::ZERO {
            if let Some(sink) = self.config.base_fee_sink_for_block(block_number) {
                info!(
                    target: "ev-reth",
                    fee_sink = ?sink,
                    block_number,
                    "Suggested fee recipient missing; defaulting to base-fee sink"
                );
                fee_recipient = sink;
            }
        }

        let evolve_attrs = EvolvePayloadAttributes::new(
            vec![],
            Some(effective_gas_limit),
            attributes.timestamp(),
            attributes.prev_randao(),
            fee_recipient,
            attributes.parent(),
            block_number,
        );

        // Build empty payload - use spawn_blocking for async work.
        let evolve_builder = self.evolve_builder.clone();
        let sealed_block = tokio::task::block_in_place(|| {
            Handle::current().block_on(evolve_builder.build_payload(evolve_attrs))
        })
        .map_err(PayloadBuilderError::other)?;

        let gas_used = sealed_block.gas_used;
        Ok(EvBuiltPayload::new(
            attributes.payload_id(),
            Arc::new(sealed_block),
            U256::from(gas_used),
            None,
        ))
    }

    /// Determines how to handle a request for a payload that is currently being built.
    ///
    /// This will always await the in-progress job, preventing a race with a new build.
    /// This is the recommended behavior to prevent redundant payload builds.
    fn on_missing_payload(
        &self,
        _args: BuildArguments<Self::Attributes, Self::BuiltPayload>,
    ) -> MissingPayloadBehaviour<Self::BuiltPayload> {
        MissingPayloadBehaviour::AwaitInProgress
    }
}
