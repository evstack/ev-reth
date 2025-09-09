//! Rollkit node binary with standard reth CLI support and rollkit payload builder integration.
//!
//! This node supports all standard reth CLI flags and functionality, with a customized
//! payload builder that accepts transactions via engine API payload attributes.

#![allow(missing_docs, rustdoc::missing_crate_level_docs)]

pub mod attributes;
pub mod builder;
pub mod error;
pub mod validator;

use alloy_rpc_types::engine::{
    ExecutionData, ExecutionPayloadEnvelopeV2, ExecutionPayloadEnvelopeV3,
    ExecutionPayloadEnvelopeV4, ExecutionPayloadEnvelopeV5, ExecutionPayloadV1,
};
use clap::Parser;
use evolve_ev_reth::{
    config::RollkitConfig,
    consensus::RollkitConsensusBuilder,
    rpc::txpool::{RollkitTxpoolApiImpl, RollkitTxpoolApiServer},
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
        node::{EthereumExecutorBuilder, EthereumNetworkBuilder, EthereumPoolBuilder},
        EthereumEthApiBuilder,
    },
    primitives::SealedBlock,
};
use reth_ethereum_cli::{chainspec::EthereumChainSpecParser, Cli};
use reth_payload_builder::EthBuiltPayload;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    attributes::{RollkitEnginePayloadAttributes, RollkitEnginePayloadBuilderAttributes},
    builder::{RollkitArgs, RollkitPayloadBuilderBuilder},
    validator::RollkitEngineValidatorBuilder,
};

#[global_allocator]
static ALLOC: reth_cli_util::allocator::Allocator = reth_cli_util::allocator::new_allocator();

/// Rollkit engine types - uses custom payload attributes that support transactions
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct RollkitEngineTypes;

impl PayloadTypes for RollkitEngineTypes {
    type ExecutionData = ExecutionData;
    type BuiltPayload = EthBuiltPayload;
    type PayloadAttributes = RollkitEnginePayloadAttributes;
    type PayloadBuilderAttributes = RollkitEnginePayloadBuilderAttributes;

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

impl EngineTypes for RollkitEngineTypes {
    type ExecutionPayloadEnvelopeV1 = ExecutionPayloadV1;
    type ExecutionPayloadEnvelopeV2 = ExecutionPayloadEnvelopeV2;
    type ExecutionPayloadEnvelopeV3 = ExecutionPayloadEnvelopeV3;
    type ExecutionPayloadEnvelopeV4 = ExecutionPayloadEnvelopeV4;
    type ExecutionPayloadEnvelopeV5 = ExecutionPayloadEnvelopeV5;
}

/// Rollkit node type
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct RollkitNode {
    /// Rollkit-specific arguments
    pub args: RollkitArgs,
}

impl RollkitNode {
    /// Create a new rollkit node with the given arguments
    pub const fn new(args: RollkitArgs) -> Self {
        Self { args }
    }
}

impl NodeTypes for RollkitNode {
    type Primitives = reth_ethereum::EthPrimitives;
    type ChainSpec = ChainSpec;
    type Storage = reth_ethereum::provider::EthStorage;
    type Payload = RollkitEngineTypes;
}

/// Rollkit node addons configuring RPC types with custom engine validator
pub type RollkitNodeAddOns<N> = RpcAddOns<N, EthereumEthApiBuilder, RollkitEngineValidatorBuilder>;

impl<N> Node<N> for RollkitNode
where
    N: FullNodeTypes<Types = Self>,
{
    type ComponentsBuilder = ComponentsBuilder<
        N,
        EthereumPoolBuilder,
        BasicPayloadServiceBuilder<RollkitPayloadBuilderBuilder>,
        EthereumNetworkBuilder,
        EthereumExecutorBuilder,
        RollkitConsensusBuilder,
    >;
    type AddOns = RollkitNodeAddOns<NodeAdapter<N>>;

    fn components_builder(&self) -> Self::ComponentsBuilder {
        ComponentsBuilder::default()
            .node_types::<N>()
            .pool(EthereumPoolBuilder::default())
            .executor(EthereumExecutorBuilder::default())
            .payload(BasicPayloadServiceBuilder::new(
                RollkitPayloadBuilderBuilder::new(&self.args),
            ))
            .network(EthereumNetworkBuilder::default())
            .consensus(RollkitConsensusBuilder::default())
    }

    fn add_ons(&self) -> Self::AddOns {
        RollkitNodeAddOns::default()
    }
}

fn main() {
    info!("=== EV-RETH NODE STARTING ===");

    reth_cli_util::sigsegv_handler::install();

    // Enable backtraces unless a RUST_BACKTRACE value has already been explicitly provided.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    if let Err(err) = Cli::<EthereumChainSpecParser, RollkitArgs>::parse().run(
        async move |builder, rollkit_args| {
            info!("=== EV-RETH: Starting with args: {:?} ===", rollkit_args);
            info!("=== EV-RETH: EV-node mode enabled ===");
            info!("=== EV-RETH: Using custom payload builder with transaction support ===");

            let handle = builder
                .node(RollkitNode::new(rollkit_args))
                .extend_rpc_modules(move |ctx| {
                    // Build custom txpool RPC
                    let rollkit_txpool = RollkitTxpoolApiImpl::new(
                        ctx.pool().clone(),
                        RollkitConfig::default().max_txpool_bytes,
                    );

                    // Merge into all enabled transports (HTTP / WS)
                    ctx.modules.merge_configured(rollkit_txpool.into_rpc())?;
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
