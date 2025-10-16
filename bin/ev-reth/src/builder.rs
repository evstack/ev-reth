use alloy_primitives::{Address, U256};
use clap::Parser;
use ev_node::{EvolvePayloadBuilder, EvolvePayloadBuilderConfig};
use evolve_ev_reth::EvolvePayloadAttributes;
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
    TransactionSigned,
};
use reth_payload_builder::{EthBuiltPayload, PayloadBuilderError};
use reth_provider::HeaderProvider;
use reth_revm::cached::CachedReads;
use std::sync::Arc;
use tracing::info;

use crate::{
    attributes::EvolveEnginePayloadBuilderAttributes, executor::EvolveEvmConfig, EvolveEngineTypes,
};
use evolve_ev_reth::config::set_current_block_gas_limit;

#[derive(Debug, Clone, Default, Parser)]
pub struct EvolveArgs {
    /// Enable Evolve mode for the node (enabled by default)
    #[arg(
        long = "ev-reth.enable",
        default_value = "true",
        help = "Enable Evolve integration for transaction processing via Engine API"
    )]
    pub enable_evolve: bool,

    /// Enable the native mint precompile.
    #[arg(long)]
    pub enable_mint_precompile: bool,
}

/// Evolve payload service builder that integrates with the evolve payload builder
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct EvolvePayloadBuilderBuilder {
    config: EvolvePayloadBuilderConfig,
}

impl EvolvePayloadBuilderBuilder {
    /// Create a new builder with evolve args
    pub fn new(_args: &EvolveArgs) -> Self {
        let config = EvolvePayloadBuilderConfig::new();
        info!("Created Evolve payload builder with config: {:?}", config);
        Self { config }
    }
}

impl Default for EvolvePayloadBuilderBuilder {
    fn default() -> Self {
        Self::new(&EvolveArgs::default())
    }
}

/// The evolve engine payload builder that integrates with the evolve payload builder
#[derive(Debug, Clone)]
pub struct EvolveEnginePayloadBuilder<Pool, Client>
where
    Pool: Clone,
    Client: Clone,
{
    pub(crate) evolve_builder: Arc<EvolvePayloadBuilder<Client>>,
    #[allow(dead_code)]
    pub(crate) pool: Pool,
    #[allow(dead_code)]
    pub(crate) config: EvolvePayloadBuilderConfig,
}

impl<Node, Pool> PayloadBuilderBuilder<Node, Pool, EvolveEvmConfig> for EvolvePayloadBuilderBuilder
where
    Node: FullNodeTypes<
        Types: NodeTypes<
            Payload = EvolveEngineTypes,
            ChainSpec = ChainSpec,
            Primitives = reth_ethereum::EthPrimitives,
        >,
    >,
    Pool: TransactionPool<Transaction: PoolTransaction<Consensus = TransactionSigned>>
        + Unpin
        + 'static,
{
    type PayloadBuilder = EvolveEnginePayloadBuilder<Pool, Node::Provider>;

    async fn build_payload_builder(
        self,
        ctx: &BuilderContext<Node>,
        pool: Pool,
        evm_config: EvolveEvmConfig,
    ) -> eyre::Result<Self::PayloadBuilder> {
        let chain_spec = ctx.chain_spec();
        let mut config = EvolvePayloadBuilderConfig::from_chain_spec(&chain_spec)?;

        if self.config.base_fee_sink.is_some() {
            config.base_fee_sink = self.config.base_fee_sink;
        }

        config.validate()?;

        let evolve_builder = Arc::new(EvolvePayloadBuilder::new(
            Arc::new(ctx.provider().clone()),
            evm_config,
            config.clone(),
        ));

        Ok(EvolveEnginePayloadBuilder {
            evolve_builder,
            pool,
            config,
        })
    }
}

impl<Pool, Client> PayloadBuilder for EvolveEnginePayloadBuilder<Pool, Client>
where
    Client: reth_ethereum::provider::StateProviderFactory
        + ChainSpecProvider<ChainSpec = ChainSpec>
        + HeaderProvider<Header = Header>
        + Clone
        + Send
        + Sync
        + 'static,
    Pool: TransactionPool<Transaction: PoolTransaction<Consensus = TransactionSigned>>,
{
    type Attributes = EvolveEnginePayloadBuilderAttributes;
    type BuiltPayload = EthBuiltPayload;

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

        // Convert Engine API attributes to Evolve payload attributes
        // If no gas_limit provided, default to the parent header's gas limit (genesis for first block)
        let effective_gas_limit = attributes.gas_limit.unwrap_or(parent_header.gas_limit);
        // Publish effective gas limit for RPC alignment
        set_current_block_gas_limit(effective_gas_limit);

        let mut fee_recipient = attributes.suggested_fee_recipient();
        if fee_recipient == Address::ZERO {
            if let Some(sink) = self.config.base_fee_sink {
                info!(
                    target: "ev-reth",
                    fee_sink = ?sink,
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
            parent_header.number + 1,
        );

        // Build the payload using the evolve payload builder - use spawn_blocking for async work
        let evolve_builder = self.evolve_builder.clone();
        let sealed_block = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(evolve_builder.build_payload(evolve_attrs))
        })
        .map_err(PayloadBuilderError::other)?;

        info!(
            "Evolve engine payload builder: built block with {} transactions, gas used: {}",
            sealed_block.transaction_count(),
            sealed_block.gas_used
        );

        // Convert to EthBuiltPayload
        let gas_used = sealed_block.gas_used;
        let built_payload = EthBuiltPayload::new(
            attributes.payload_id(), // Use the proper payload ID from attributes
            Arc::new(sealed_block),
            U256::from(gas_used), // Block gas used
            None,                 // No blob sidecar for evolve
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

        // Create empty evolve attributes (no transactions)
        // If no gas_limit provided, default to the parent header's gas limit (genesis for first block)
        let effective_gas_limit = attributes.gas_limit.unwrap_or(parent_header.gas_limit);
        // Publish effective gas limit for RPC alignment
        set_current_block_gas_limit(effective_gas_limit);

        let mut fee_recipient = attributes.suggested_fee_recipient();
        if fee_recipient == Address::ZERO {
            if let Some(sink) = self.config.base_fee_sink {
                info!(
                    target: "ev-reth",
                    fee_sink = ?sink,
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
            parent_header.number + 1,
        );

        // Build empty payload - use spawn_blocking for async work
        let evolve_builder = self.evolve_builder.clone();
        let sealed_block = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(evolve_builder.build_payload(evolve_attrs))
        })
        .map_err(PayloadBuilderError::other)?;

        let gas_used = sealed_block.gas_used;
        Ok(EthBuiltPayload::new(
            attributes.payload_id(),
            Arc::new(sealed_block),
            U256::from(gas_used),
            None,
        ))
    }

    /// Determines how to handle a request for a payload that is currently being built.
    ///
    /// This will always await the in-progress job, preventing a race with a new build.
    /// This is the recommended behavior to prevent redundant payload builds
    fn on_missing_payload(
        &self,
        _args: BuildArguments<Self::Attributes, Self::BuiltPayload>,
    ) -> MissingPayloadBehaviour<Self::BuiltPayload> {
        MissingPayloadBehaviour::AwaitInProgress
    }
}
