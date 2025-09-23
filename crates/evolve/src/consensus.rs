//! Evolve custom consensus implementation that allows same timestamps across blocks.

use reth_chainspec::ChainSpec;
use reth_consensus::{Consensus, ConsensusError, FullConsensus, HeaderValidator};
use reth_consensus_common::validation::validate_body_against_header;
use reth_ethereum::node::builder::{components::ConsensusBuilder, BuilderContext};
use reth_ethereum_consensus::EthBeaconConsensus;
use reth_ethereum_primitives::{Block, BlockBody, EthPrimitives, Receipt};
use reth_execution_types::BlockExecutionResult;
use reth_node_api::{FullNodeTypes, NodeTypes};
use reth_primitives::{RecoveredBlock, SealedBlock, SealedHeader};
use std::sync::Arc;

/// Builder for `EvolveConsensus`
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct EvolveConsensusBuilder;

impl EvolveConsensusBuilder {
    /// Create a new `EvolveConsensusBuilder`
    pub const fn new() -> Self {
        Self
    }

    /// Build the consensus implementation
    pub fn build(chain_spec: Arc<ChainSpec>) -> Arc<EvolveConsensus> {
        Arc::new(EvolveConsensus::new(chain_spec))
    }
}

impl<Node> ConsensusBuilder<Node> for EvolveConsensusBuilder
where
    Node: FullNodeTypes,
    Node::Types: NodeTypes<ChainSpec = ChainSpec, Primitives = EthPrimitives>,
{
    type Consensus = Arc<dyn FullConsensus<EthPrimitives, Error = ConsensusError>>;

    async fn build_consensus(self, ctx: &BuilderContext<Node>) -> eyre::Result<Self::Consensus> {
        Ok(Arc::new(EvolveConsensus::new(ctx.chain_spec())) as Self::Consensus)
    }
}

/// Evolve consensus implementation that allows blocks with the same timestamp.
///
/// This consensus implementation wraps the standard Ethereum beacon consensus
/// but modifies the timestamp validation to allow multiple blocks to have the
/// same timestamp, which is required for Evolve's operation.
#[derive(Debug, Clone)]
pub struct EvolveConsensus {
    /// Inner Ethereum beacon consensus for standard validation
    inner: EthBeaconConsensus<ChainSpec>,
}

impl EvolveConsensus {
    /// Create a new Evolve consensus instance
    pub const fn new(chain_spec: Arc<ChainSpec>) -> Self {
        let inner = EthBeaconConsensus::new(chain_spec);
        Self { inner }
    }
}

impl HeaderValidator for EvolveConsensus {
    fn validate_header(&self, header: &SealedHeader) -> Result<(), ConsensusError> {
        // Use inner consensus for basic header validation
        self.inner.validate_header(header)
    }

    fn validate_header_against_parent(
        &self,
        header: &SealedHeader,
        parent: &SealedHeader,
    ) -> Result<(), ConsensusError> {
        match self.inner.validate_header_against_parent(header, parent) {
            Ok(()) => Ok(()),
            // upstream the check is that its greater than the parent's timestamp, if not we get
            // TimestampIsInPast we check if the timestamp is equal to the parent's timestamp, if so we
            // allow it
            Err(ConsensusError::TimestampIsInPast { .. }) => {
                if header.timestamp == parent.timestamp {
                    Ok(())
                } else {
                    Err(ConsensusError::TimestampIsInPast {
                        parent_timestamp: parent.timestamp,
                        timestamp: header.timestamp,
                    })
                }
            }
            Err(e) => Err(e),
        }
    }
}

impl Consensus<Block> for EvolveConsensus {
    type Error = ConsensusError;

    fn validate_body_against_header(
        &self,
        body: &BlockBody,
        header: &SealedHeader,
    ) -> Result<(), Self::Error> {
        validate_body_against_header(body, header.header())
    }

    fn validate_block_pre_execution(&self, block: &SealedBlock) -> Result<(), Self::Error> {
        // Use inner consensus for pre-execution validation
        self.inner.validate_block_pre_execution(block)
    }
}

impl FullConsensus<EthPrimitives> for EvolveConsensus {
    fn validate_block_post_execution(
        &self,
        block: &RecoveredBlock<Block>,
        result: &BlockExecutionResult<Receipt>,
    ) -> Result<(), ConsensusError> {
        <EthBeaconConsensus<ChainSpec> as FullConsensus<EthPrimitives>>::validate_block_post_execution(&self.inner, block, result)
    }
}
