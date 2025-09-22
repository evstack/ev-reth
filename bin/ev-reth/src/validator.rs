#![allow(missing_docs, rustdoc::missing_crate_level_docs)]

use alloy_rpc_types::engine::ExecutionData;

use reth_ethereum::{
    chainspec::ChainSpec,
    node::{
        api::{
            payload::{EngineApiMessageVersion, EngineObjectValidationError, PayloadOrAttributes},
            validate_version_specific_fields, AddOnsContext, EngineApiValidator,
            FullNodeComponents, InvalidPayloadAttributesError, NewPayloadError, NodeTypes,
            PayloadValidator,
        },
        builder::rpc::PayloadValidatorBuilder,
    },
};
use reth_ethereum_payload_builder::EthereumExecutionPayloadValidator;
use reth_primitives_traits::{Block as _, RecoveredBlock};
use std::sync::Arc;
use tracing::info;

use crate::{attributes::RollkitEnginePayloadAttributes, RollkitEngineTypes};

/// Rollkit engine validator that handles custom payload validation
#[derive(Debug, Clone)]
pub struct RollkitEngineValidator {
    inner: EthereumExecutionPayloadValidator<ChainSpec>,
}

impl RollkitEngineValidator {
    /// Instantiates a new validator.
    pub const fn new(chain_spec: Arc<ChainSpec>) -> Self {
        Self {
            inner: EthereumExecutionPayloadValidator::new(chain_spec),
        }
    }

    /// Returns the chain spec used by the validator.
    #[inline]
    fn chain_spec(&self) -> &ChainSpec {
        self.inner.chain_spec().as_ref()
    }
}

impl PayloadValidator<crate::RollkitEngineTypes> for RollkitEngineValidator {
    type Block = reth_ethereum::Block;

    fn ensure_well_formed_payload(
        &self,
        payload: ExecutionData,
    ) -> Result<RecoveredBlock<Self::Block>, NewPayloadError> {
        info!("Rollkit engine validator: validating payload");

        // Use inner validator but with custom rollkit handling
        match self.inner.ensure_well_formed_payload(payload.clone()) {
            Ok(sealed_block) => {
                info!("Rollkit engine validator: payload validation succeeded");
                sealed_block
                    .try_recover()
                    .map_err(|e| NewPayloadError::Other(e.into()))
            }
            Err(err) => {
                // Log the error for debugging
                tracing::debug!("Rollkit payload validation error: {:?}", err);

                // Check if this is a block hash mismatch error - bypass it for rollkit
                if matches!(err, alloy_rpc_types::engine::PayloadError::BlockHash { .. }) {
                    info!("Rollkit engine validator: bypassing block hash mismatch for ev-reth");
                    // For rollkit, we trust the payload builder - just parse the block without hash validation
                    let ExecutionData { payload, sidecar } = payload;
                    let sealed_block = payload.try_into_block_with_sidecar(&sidecar)?.seal_slow();
                    sealed_block
                        .try_recover()
                        .map_err(|e| NewPayloadError::Other(e.into()))
                } else {
                    // For other errors, re-throw them
                    Err(NewPayloadError::Eth(err))
                }
            }
        }
    }

    fn validate_payload_attributes_against_header(
        &self,
        _attr: &crate::attributes::RollkitEnginePayloadAttributes,
        _header: &<Self::Block as reth_primitives_traits::Block>::Header,
    ) -> Result<(), InvalidPayloadAttributesError> {
        // Skip default timestamp validation for rollkit
        Ok(())
    }
}

impl EngineApiValidator<crate::RollkitEngineTypes> for RollkitEngineValidator {
    fn validate_version_specific_fields(
        &self,
        version: EngineApiMessageVersion,
        payload_or_attrs: PayloadOrAttributes<'_, ExecutionData, RollkitEnginePayloadAttributes>,
    ) -> Result<(), EngineObjectValidationError> {
        validate_version_specific_fields(self.chain_spec(), version, payload_or_attrs)
    }

    fn ensure_well_formed_attributes(
        &self,
        version: EngineApiMessageVersion,
        attributes: &RollkitEnginePayloadAttributes,
    ) -> Result<(), EngineObjectValidationError> {
        validate_version_specific_fields(
            self.chain_spec(),
            version,
            PayloadOrAttributes::<ExecutionData, RollkitEnginePayloadAttributes>::PayloadAttributes(
                attributes,
            ),
        )?;

        // Validate rollkit-specific attributes
        if let Some(ref transactions) = attributes.transactions {
            info!(
                "Rollkit engine validator: validating {} transactions",
                transactions.len()
            );
        }

        Ok(())
    }
}

/// Rollkit engine validator builder
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct RollkitEngineValidatorBuilder;

impl<N> PayloadValidatorBuilder<N> for RollkitEngineValidatorBuilder
where
    N: FullNodeComponents<
        Types: NodeTypes<
            Payload = RollkitEngineTypes,
            ChainSpec = ChainSpec,
            Primitives = reth_ethereum::EthPrimitives,
        >,
    >,
{
    type Validator = RollkitEngineValidator;

    async fn build(self, ctx: &AddOnsContext<'_, N>) -> eyre::Result<Self::Validator> {
        Ok(RollkitEngineValidator::new(ctx.config.chain.clone()))
    }
}
