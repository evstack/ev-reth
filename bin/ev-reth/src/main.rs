//! Evolve node binary with standard reth CLI support and evolve payload builder integration.
//!
//! This node supports all standard reth CLI flags and functionality, with a customized
//! payload builder that accepts transactions via engine API payload attributes.

#![allow(missing_docs, rustdoc::missing_crate_level_docs)]

pub mod attributes;
pub mod builder;
pub mod error;
pub mod executor;
pub mod validator;

use alloy_rpc_types::engine::{
    ExecutionData, ExecutionPayloadEnvelopeV2, ExecutionPayloadEnvelopeV3,
    ExecutionPayloadEnvelopeV4, ExecutionPayloadEnvelopeV5, ExecutionPayloadV1,
};
use clap::Parser;
use evolve_ev_reth::{
    config::EvolveConfig,
    consensus::EvolveConsensusBuilder,
    rpc::txpool::{EvolveTxpoolApiImpl, EvolveTxpoolApiServer},
};
use reth_ethereum::{
    chainspec::ChainSpec,
    node::{
        api::{EngineTypes, FullNodeTypes, NodeTypes, PayloadTypes},
        builder::{
            components::{BasicPayloadServiceBuilder, ComponentsBuilder},
            rpc::RpcAddOns,
            Node, NodeAdapter,
        },
        node::{EthereumNetworkBuilder, EthereumPoolBuilder},
        EthereumEthApiBuilder,
    },
    primitives::SealedBlock,
};
use reth_ethereum_cli::{chainspec::EthereumChainSpecParser, Cli};
use reth_payload_builder::EthBuiltPayload;
use serde::{Deserialize, Serialize};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{
    attributes::{EvolveEnginePayloadAttributes, EvolveEnginePayloadBuilderAttributes},
    builder::{EvolveArgs, EvolvePayloadBuilderBuilder},
    executor::EvolveExecutorBuilder,
    validator::EvolveEngineValidatorBuilder,
};

#[global_allocator]
static ALLOC: reth_cli_util::allocator::Allocator = reth_cli_util::allocator::new_allocator();

/// Initialize reth OTLP tracing
fn init_otlp_tracing() -> eyre::Result<()> {
    // Set up tracing subscriber with reth OTLP layer
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(reth_tracing_otlp::layer("ev-reth"))
        .init();

    info!("Reth OTLP tracing initialized for service: ev-reth");
    Ok(())
}

/// Evolve engine types - uses custom payload attributes that support transactions
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct EvolveEngineTypes;

impl PayloadTypes for EvolveEngineTypes {
    type ExecutionData = ExecutionData;
    type BuiltPayload = EthBuiltPayload;
    type PayloadAttributes = EvolveEnginePayloadAttributes;
    type PayloadBuilderAttributes = EvolveEnginePayloadBuilderAttributes;

    fn block_to_payload(
        block: SealedBlock<
            <<Self::BuiltPayload as reth_ethereum::node::api::BuiltPayload>::Primitives as reth_ethereum::node::api::NodePrimitives>::Block,
        >,
    ) -> ExecutionData {
        let (payload, sidecar) =
            reth_ethereum::rpc::types::engine::ExecutionPayload::from_block_unchecked(
                block.hash(),
                &block.into_block(),
            );
        ExecutionData { payload, sidecar }
    }
}

impl EngineTypes for EvolveEngineTypes {
    type ExecutionPayloadEnvelopeV1 = ExecutionPayloadV1;
    type ExecutionPayloadEnvelopeV2 = ExecutionPayloadEnvelopeV2;
    type ExecutionPayloadEnvelopeV3 = ExecutionPayloadEnvelopeV3;
    type ExecutionPayloadEnvelopeV4 = ExecutionPayloadEnvelopeV4;
    type ExecutionPayloadEnvelopeV5 = ExecutionPayloadEnvelopeV5;
}

/// Evolve node type
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct EvolveNode {
    /// Evolve-specific arguments
    pub args: EvolveArgs,
}

impl EvolveNode {
    /// Create a new evolve node with the given arguments
    pub const fn new(args: EvolveArgs) -> Self {
        Self { args }
    }
}

impl NodeTypes for EvolveNode {
    type Primitives = reth_ethereum::EthPrimitives;
    type ChainSpec = ChainSpec;
    type Storage = reth_ethereum::provider::EthStorage;
    type Payload = EvolveEngineTypes;
}

/// Evolve node addons configuring RPC types with custom engine validator
pub type EvolveNodeAddOns<N> = RpcAddOns<N, EthereumEthApiBuilder, EvolveEngineValidatorBuilder>;

impl<N> Node<N> for EvolveNode
where
    N: FullNodeTypes<Types = Self>,
{
    type ComponentsBuilder = ComponentsBuilder<
        N,
        EthereumPoolBuilder,
        BasicPayloadServiceBuilder<EvolvePayloadBuilderBuilder>,
        EthereumNetworkBuilder,
        EvolveExecutorBuilder,
        EvolveConsensusBuilder,
    >;
    type AddOns = EvolveNodeAddOns<NodeAdapter<N>>;

    fn components_builder(&self) -> Self::ComponentsBuilder {
        ComponentsBuilder::default()
            .node_types::<N>()
            .pool(EthereumPoolBuilder::default())
            .executor(EvolveExecutorBuilder::default())
            .payload(BasicPayloadServiceBuilder::new(
                EvolvePayloadBuilderBuilder::new(&self.args),
            ))
            .network(EthereumNetworkBuilder::default())
            .consensus(EvolveConsensusBuilder::default())
    }

    fn add_ons(&self) -> Self::AddOns {
        EvolveNodeAddOns::default()
    }
}

fn main() {
    info!("=== EV-RETH NODE STARTING ===");

    reth_cli_util::sigsegv_handler::install();

    // Enable backtraces unless a RUST_BACKTRACE value has already been explicitly provided.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    // Initialize OTLP tracing
    if std::env::var("OTEL_SDK_DISABLED").as_deref() == Ok("false") {
        if let Err(e) = init_otlp_tracing() {
            eprintln!("Failed to initialize OTLP tracing: {:?}", e);
            eprintln!("Continuing without OTLP tracing...");
        }
    }

    if let Err(err) = Cli::<EthereumChainSpecParser, EvolveArgs>::parse().run(
        async move |builder, evolve_args| {
            info!("=== EV-RETH: Starting with args: {:?} ===", evolve_args);
            info!("=== EV-RETH: Evolve node mode enabled ===");
            info!("=== EV-RETH: Using custom payload builder with transaction support ===");
            let handle = builder
                .node(EvolveNode::new(evolve_args))
                .extend_rpc_modules(move |ctx| {
                    // Build custom txpool RPC with config + optional CLI/env override
                    let evolve_cfg = EvolveConfig::default();
                    let evolve_txpool =
                        EvolveTxpoolApiImpl::new(ctx.pool().clone(), evolve_cfg.max_txpool_bytes);

                    // Merge into all enabled transports (HTTP / WS)
                    ctx.modules.merge_configured(evolve_txpool.into_rpc())?;
                    Ok(())
                })
                .launch()
                .await?;

            info!("=== EV-RETH: Node launched successfully with ev-reth payload builder ===");
            handle.node_exit_future.await
        },
    ) {
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
}
