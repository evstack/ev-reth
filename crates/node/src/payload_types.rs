use std::sync::Arc;

use alloy_eips::eip7685::Requests;
use alloy_primitives::U256;
use alloy_rpc_types_engine::{
    BlobsBundleV1, BlobsBundleV2, ExecutionPayloadEnvelopeV2, ExecutionPayloadEnvelopeV3,
    ExecutionPayloadEnvelopeV4, ExecutionPayloadEnvelopeV5, ExecutionPayloadFieldV2,
    ExecutionPayloadV1, ExecutionPayloadV3, PayloadId,
};
use ev_primitives::EvPrimitives;
use reth_payload_builder::BlobSidecars;
use reth_payload_primitives::BuiltPayload;
use reth_primitives_traits::SealedBlock;

/// Built payload for EvPrimitives.
#[derive(Debug, Clone)]
pub struct EvBuiltPayload {
    id: PayloadId,
    block: Arc<SealedBlock<ev_primitives::Block>>,
    fees: U256,
    sidecars: BlobSidecars,
    requests: Option<Requests>,
}

/// Errors encountered when converting an EV payload into an engine API envelope.
#[derive(Debug, thiserror::Error)]
pub enum EvBuiltPayloadConversionError {
    /// EIP-7594 sidecars are not valid for this envelope version.
    #[error("unexpected EIP-7594 sidecars for this payload")]
    UnexpectedEip7594Sidecars,
    /// EIP-4844 sidecars are not valid for this envelope version.
    #[error("unexpected EIP-4844 sidecars for this payload")]
    UnexpectedEip4844Sidecars,
}

impl EvBuiltPayload {
    /// Creates a new EV built payload.
    pub const fn new(
        id: PayloadId,
        block: Arc<SealedBlock<ev_primitives::Block>>,
        fees: U256,
        requests: Option<Requests>,
    ) -> Self {
        Self {
            id,
            block,
            fees,
            requests,
            sidecars: BlobSidecars::Empty,
        }
    }

    /// Returns the payload identifier.
    pub const fn id(&self) -> PayloadId {
        self.id
    }

    /// Returns the sealed block backing this payload.
    pub fn block(&self) -> &SealedBlock<ev_primitives::Block> {
        &self.block
    }

    /// Returns the total fees for this payload.
    pub const fn fees(&self) -> U256 {
        self.fees
    }

    /// Returns the sidecar bundle.
    pub const fn sidecars(&self) -> &BlobSidecars {
        &self.sidecars
    }

    /// Attaches the provided sidecars and returns the updated payload.
    pub fn with_sidecars(mut self, sidecars: impl Into<BlobSidecars>) -> Self {
        self.sidecars = sidecars.into();
        self
    }

    /// Converts this payload into an ExecutionPayloadEnvelopeV3.
    pub fn try_into_v3(self) -> Result<ExecutionPayloadEnvelopeV3, EvBuiltPayloadConversionError> {
        let Self {
            block,
            fees,
            sidecars,
            ..
        } = self;

        let blobs_bundle = match sidecars {
            BlobSidecars::Empty => BlobsBundleV1::empty(),
            BlobSidecars::Eip4844(sidecars) => BlobsBundleV1::from(sidecars),
            BlobSidecars::Eip7594(_) => {
                return Err(EvBuiltPayloadConversionError::UnexpectedEip7594Sidecars)
            }
        };

        Ok(ExecutionPayloadEnvelopeV3 {
            execution_payload: ExecutionPayloadV3::from_block_unchecked(
                block.hash(),
                &Arc::unwrap_or_clone(block).into_block(),
            ),
            block_value: fees,
            should_override_builder: false,
            blobs_bundle,
        })
    }

    /// Converts this payload into an ExecutionPayloadEnvelopeV4.
    pub fn try_into_v4(self) -> Result<ExecutionPayloadEnvelopeV4, EvBuiltPayloadConversionError> {
        Ok(ExecutionPayloadEnvelopeV4 {
            execution_requests: self.requests.clone().unwrap_or_default(),
            envelope_inner: self.try_into()?,
        })
    }

    /// Converts this payload into an ExecutionPayloadEnvelopeV5.
    pub fn try_into_v5(self) -> Result<ExecutionPayloadEnvelopeV5, EvBuiltPayloadConversionError> {
        let Self {
            block,
            fees,
            sidecars,
            requests,
            ..
        } = self;

        let blobs_bundle = match sidecars {
            BlobSidecars::Empty => BlobsBundleV2::empty(),
            BlobSidecars::Eip7594(sidecars) => BlobsBundleV2::from(sidecars),
            BlobSidecars::Eip4844(_) => {
                return Err(EvBuiltPayloadConversionError::UnexpectedEip4844Sidecars)
            }
        };

        Ok(ExecutionPayloadEnvelopeV5 {
            execution_payload: ExecutionPayloadV3::from_block_unchecked(
                block.hash(),
                &Arc::unwrap_or_clone(block).into_block(),
            ),
            block_value: fees,
            should_override_builder: false,
            blobs_bundle,
            execution_requests: requests.unwrap_or_default(),
        })
    }
}

impl BuiltPayload for EvBuiltPayload {
    type Primitives = EvPrimitives;

    fn block(&self) -> &SealedBlock<ev_primitives::Block> {
        &self.block
    }

    fn fees(&self) -> U256 {
        self.fees
    }

    fn requests(&self) -> Option<Requests> {
        self.requests.clone()
    }
}

impl From<EvBuiltPayload> for ExecutionPayloadV1 {
    fn from(value: EvBuiltPayload) -> Self {
        Self::from_block_unchecked(
            value.block().hash(),
            &Arc::unwrap_or_clone(value.block).into_block(),
        )
    }
}

impl From<EvBuiltPayload> for ExecutionPayloadEnvelopeV2 {
    fn from(value: EvBuiltPayload) -> Self {
        let EvBuiltPayload { block, fees, .. } = value;

        Self {
            block_value: fees,
            execution_payload: ExecutionPayloadFieldV2::from_block_unchecked(
                block.hash(),
                &Arc::unwrap_or_clone(block).into_block(),
            ),
        }
    }
}

impl TryFrom<EvBuiltPayload> for ExecutionPayloadEnvelopeV3 {
    type Error = EvBuiltPayloadConversionError;

    fn try_from(value: EvBuiltPayload) -> Result<Self, Self::Error> {
        value.try_into_v3()
    }
}

impl TryFrom<EvBuiltPayload> for ExecutionPayloadEnvelopeV4 {
    type Error = EvBuiltPayloadConversionError;

    fn try_from(value: EvBuiltPayload) -> Result<Self, Self::Error> {
        value.try_into_v4()
    }
}

impl TryFrom<EvBuiltPayload> for ExecutionPayloadEnvelopeV5 {
    type Error = EvBuiltPayloadConversionError;

    fn try_from(value: EvBuiltPayload) -> Result<Self, Self::Error> {
        value.try_into_v5()
    }
}
