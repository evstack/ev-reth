//! Node wiring for ev-reth, including payload types and component assembly.

use alloy_rpc_types::engine::{
    ExecutionData, ExecutionPayloadEnvelopeV2, ExecutionPayloadEnvelopeV3,
    ExecutionPayloadEnvelopeV4, ExecutionPayloadEnvelopeV5, ExecutionPayloadV1,
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
use reth_payload_builder::EthBuiltPayload;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    attributes::{EvolveEnginePayloadAttributes, EvolveEnginePayloadBuilderAttributes},
    executor::EvolveExecutorBuilder,
    payload_service::EvolvePayloadBuilderBuilder,
    validator::EvolveEngineValidatorBuilder,
};

/// Evolve engine types - uses custom payload attributes that support transactions.
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

/// Evolve node type.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct EvolveNode {}

impl EvolveNode {
    /// Create a new evolve node with the given arguments.
    pub const fn new() -> Self {
        Self {}
    }
}

impl NodeTypes for EvolveNode {
    type Primitives = reth_ethereum::EthPrimitives;
    type ChainSpec = ChainSpec;
    type Storage = reth_ethereum::provider::EthStorage;
    type Payload = EvolveEngineTypes;
}

/// Evolve node addons configuring RPC types with custom engine validator.
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
        evolve_ev_reth::consensus::EvolveConsensusBuilder,
    >;
    type AddOns = EvolveNodeAddOns<NodeAdapter<N>>;

    fn components_builder(&self) -> Self::ComponentsBuilder {
        ComponentsBuilder::default()
            .node_types::<N>()
            .pool(EthereumPoolBuilder::default())
            .executor(EvolveExecutorBuilder::default())
            .payload(BasicPayloadServiceBuilder::new(
                EvolvePayloadBuilderBuilder::new(),
            ))
            .network(EthereumNetworkBuilder::default())
            .consensus(evolve_ev_reth::consensus::EvolveConsensusBuilder::default())
    }

    fn add_ons(&self) -> Self::AddOns {
        EvolveNodeAddOns::default()
    }
}

/// Helper logging to announce node startup with args.
pub fn log_startup() {
    info!("=== EV-RETH: Evolve node mode enabled ===");
    info!("=== EV-RETH: Using custom payload builder with transaction support ===");
}
