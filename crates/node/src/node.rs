//! Node wiring for ev-reth, including payload types and component assembly.

use alloy_rpc_types::engine::{
    ExecutionData, ExecutionPayloadEnvelopeV2, ExecutionPayloadEnvelopeV3,
    ExecutionPayloadEnvelopeV4, ExecutionPayloadEnvelopeV5, ExecutionPayloadEnvelopeV6,
    ExecutionPayloadV1,
};
use ev_primitives::EvPrimitives;
use reth_engine_local::LocalPayloadAttributesBuilder;
use reth_ethereum::{
    chainspec::ChainSpec,
    node::{
        api::{EngineTypes, FullNodeComponents, FullNodeTypes, NodeTypes, PayloadTypes},
        builder::{
            components::{BasicPayloadServiceBuilder, ComponentsBuilder},
            rpc::RpcAddOns,
            DebugNode, Node, NodeAdapter,
        },
        node::EthereumNetworkBuilder,
    },
};
use reth_payload_primitives::PayloadAttributesBuilder;
use reth_primitives_traits::SealedBlock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

use crate::{
    attributes::{EvolveEnginePayloadAttributes, EvolveEnginePayloadBuilderAttributes},
    executor::EvolveExecutorBuilder,
    payload_service::EvolvePayloadBuilderBuilder,
    payload_types::EvBuiltPayload,
    rpc::EvEthApiBuilder,
    txpool::EvolvePoolBuilder,
    validator::EvolveEngineValidatorBuilder,
};

/// Evolve engine types - uses custom payload attributes that support transactions.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[non_exhaustive]
pub struct EvolveEngineTypes;

impl PayloadTypes for EvolveEngineTypes {
    type ExecutionData = ExecutionData;
    type BuiltPayload = EvBuiltPayload;
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
    type ExecutionPayloadEnvelopeV6 = ExecutionPayloadEnvelopeV6;
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
    type Primitives = EvPrimitives;
    type ChainSpec = ChainSpec;
    type Storage = reth_ethereum::provider::EthStorage<ev_primitives::TransactionSigned>;
    type Payload = EvolveEngineTypes;
}

/// Evolve node addons configuring RPC types with custom engine validator.
pub type EvolveNodeAddOns<N> = RpcAddOns<N, EvEthApiBuilder, EvolveEngineValidatorBuilder>;

impl<N> Node<N> for EvolveNode
where
    N: FullNodeTypes<Types = Self>,
{
    type ComponentsBuilder = ComponentsBuilder<
        N,
        EvolvePoolBuilder,
        BasicPayloadServiceBuilder<EvolvePayloadBuilderBuilder>,
        EthereumNetworkBuilder,
        EvolveExecutorBuilder,
        evolve_ev_reth::consensus::EvolveConsensusBuilder,
    >;
    type AddOns = EvolveNodeAddOns<NodeAdapter<N>>;

    fn components_builder(&self) -> Self::ComponentsBuilder {
        ComponentsBuilder::default()
            .node_types::<N>()
            .pool(EvolvePoolBuilder::default())
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

impl<N: FullNodeComponents<Types = Self>> DebugNode<N> for EvolveNode {
    type RpcBlock = alloy_rpc_types::Block;

    fn rpc_to_primitive_block(rpc_block: Self::RpcBlock) -> ev_primitives::Block {
        let eth_block: reth_ethereum_primitives::Block =
            rpc_block.into_consensus().convert_transactions();
        eth_block.map_transactions(ev_primitives::EvTxEnvelope::Ethereum)
    }

    fn local_payload_attributes_builder(
        chain_spec: &Self::ChainSpec,
    ) -> impl PayloadAttributesBuilder<<Self::Payload as PayloadTypes>::PayloadAttributes> {
        LocalPayloadAttributesBuilder::new(Arc::new(chain_spec.clone()))
    }
}

/// Helper logging to announce node startup with args.
pub fn log_startup() {
    info!("=== EV-RETH: Evolve node mode enabled ===");
    info!("=== EV-RETH: Using custom payload builder with transaction support ===");
}
