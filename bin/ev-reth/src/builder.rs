use alloy_primitives::U256;
use clap::Parser;
use ev_node::{RollkitPayloadBuilder, RollkitPayloadBuilderConfig};
use evolve_ev_reth::RollkitPayloadAttributes;
use reth_basic_payload_builder::{
    BuildArguments, BuildOutcome, HeaderForPayload, MissingPayloadBehaviour, PayloadBuilder,
    PayloadConfig,
};
use reth_ethereum::{
    chainspec::{ChainSpec, ChainSpecProvider},
    node::{
        api::{payload::PayloadBuilderAttributes, FullNodeTypes, NodeTypes},
        builder::{components::PayloadBuilderBuilder, BuilderContext},
        EthEvmConfig,
    },
    pool::{PoolTransaction, TransactionPool},
    primitives::Header,
    TransactionSigned,
};
use reth_payload_builder::{EthBuiltPayload, PayloadBuilderError};
use reth_provider::HeaderProvider;
use reth_revm::cached::CachedReads;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

use crate::{attributes::RollkitEnginePayloadBuilderAttributes, RollkitEngineTypes};

/// Rollkit-specific command line arguments
#[derive(Debug, Clone, Parser, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RollkitArgs {
    /// Enable Rollkit mode for the node (enabled by default)
    #[arg(
        long = "ev-reth.enable",
        default_value = "true",
        help = "Enable Evolve integration for transaction processing via Engine API"
    )]
    pub enable_rollkit: bool,

    /// Maximum cumulative gas to return from txpoolExt_getTxs
    /// 0 disables the gas constraint and uses bytes-only selection
    #[arg(
        long = "ev-reth.txpool-max-gas",
        env = "EV_RETH_TXPOOL_MAX_GAS",
        value_parser = clap::value_parser!(u64),
        help = "Override max cumulative gas returned by txpoolExt_getTxs (default: 10,000,000)"
    )]
    pub txpool_max_gas: Option<u64>,
}

/// Rollkit payload service builder that integrates with the rollkit payload builder
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RollkitPayloadBuilderBuilder {
    config: RollkitPayloadBuilderConfig,
}

impl RollkitPayloadBuilderBuilder {
    /// Create a new builder with rollkit args
    pub fn new(_args: &RollkitArgs) -> Self {
        let config = RollkitPayloadBuilderConfig::new();
        info!("Created Rollkit payload builder with config: {:?}", config);
        Self { config }
    }
}

impl Default for RollkitPayloadBuilderBuilder {
    fn default() -> Self {
        Self::new(&RollkitArgs::default())
    }
}

/// The rollkit engine payload builder that integrates with the rollkit payload builder
#[derive(Debug, Clone)]
pub struct RollkitEnginePayloadBuilder<Pool, Client>
where
    Pool: Clone,
    Client: Clone,
{
    pub(crate) rollkit_builder: Arc<RollkitPayloadBuilder<Client>>,
    #[allow(dead_code)]
    pub(crate) pool: Pool,
    #[allow(dead_code)]
    pub(crate) config: RollkitPayloadBuilderConfig,
}

impl<Node, Pool> PayloadBuilderBuilder<Node, Pool, EthEvmConfig> for RollkitPayloadBuilderBuilder
where
    Node: FullNodeTypes<
        Types: NodeTypes<
            Payload = RollkitEngineTypes,
            ChainSpec = ChainSpec,
            Primitives = reth_ethereum::EthPrimitives,
        >,
    >,
    Pool: TransactionPool<Transaction: PoolTransaction<Consensus = TransactionSigned>>
        + Unpin
        + 'static,
{
    type PayloadBuilder = RollkitEnginePayloadBuilder<Pool, Node::Provider>;

    async fn build_payload_builder(
        self,
        ctx: &BuilderContext<Node>,
        pool: Pool,
        evm_config: EthEvmConfig,
    ) -> eyre::Result<Self::PayloadBuilder> {
        let rollkit_builder = Arc::new(RollkitPayloadBuilder::new(
            Arc::new(ctx.provider().clone()),
            evm_config,
        ));

        Ok(RollkitEnginePayloadBuilder {
            rollkit_builder,
            pool,
            config: self.config,
        })
    }
}

impl<Pool, Client> PayloadBuilder for RollkitEnginePayloadBuilder<Pool, Client>
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
    type Attributes = RollkitEnginePayloadBuilderAttributes;
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
            "Rollkit engine payload builder: building payload with {} transactions",
            attributes.transactions.len()
        );

        // Convert Engine API attributes to Rollkit payload attributes
        let rollkit_attrs = RollkitPayloadAttributes::new(
            attributes.transactions.clone(),
            attributes.gas_limit,
            attributes.timestamp(),
            attributes.prev_randao(),
            attributes.suggested_fee_recipient(),
            attributes.parent(),
            parent_header.number + 1,
        );

        // Build the payload using the rollkit payload builder - use spawn_blocking for async work
        let rollkit_builder = self.rollkit_builder.clone();
        let sealed_block = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(rollkit_builder.build_payload(rollkit_attrs))
        })
        .map_err(PayloadBuilderError::other)?;

        info!(
            "Rollkit engine payload builder: built block with {} transactions, gas used: {}",
            sealed_block.transaction_count(),
            sealed_block.gas_used
        );

        // Convert to EthBuiltPayload
        let gas_used = sealed_block.gas_used;
        let built_payload = EthBuiltPayload::new(
            attributes.payload_id(), // Use the proper payload ID from attributes
            Arc::new(sealed_block),
            U256::from(gas_used), // Block gas used
            None,                 // No blob sidecar for rollkit
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

        info!("Rollkit engine payload builder: building empty payload");

        // Create empty rollkit attributes (no transactions)
        let rollkit_attrs = RollkitPayloadAttributes::new(
            vec![],
            attributes.gas_limit,
            attributes.timestamp(),
            attributes.prev_randao(),
            attributes.suggested_fee_recipient(),
            attributes.parent(),
            parent_header.number + 1,
        );

        // Build empty payload - use spawn_blocking for async work
        let rollkit_builder = self.rollkit_builder.clone();
        let sealed_block = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(rollkit_builder.build_payload(rollkit_attrs))
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
