#![allow(missing_docs, rustdoc::missing_crate_level_docs)]

use std::sync::Arc;

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
use tracing::info;

use crate::{
    attributes::EvolveEnginePayloadAttributes, config::EvolvePayloadBuilderConfig,
    node::EvolveEngineTypes,
};

/// Evolve engine validator that handles custom payload validation.
#[derive(Debug, Clone)]
pub struct EvolveEngineValidator {
    inner: EthereumExecutionPayloadValidator<ChainSpec>,
    config: EvolvePayloadBuilderConfig,
}

impl EvolveEngineValidator {
    /// Instantiates a new validator.
    pub const fn new(chain_spec: Arc<ChainSpec>, config: EvolvePayloadBuilderConfig) -> Self {
        Self {
            inner: EthereumExecutionPayloadValidator::new(chain_spec),
            config,
        }
    }

    /// Returns the chain spec used by the validator.
    #[inline]
    fn chain_spec(&self) -> &ChainSpec {
        self.inner.chain_spec().as_ref()
    }
}

impl PayloadValidator<EvolveEngineTypes> for EvolveEngineValidator {
    type Block = reth_ethereum::Block;

    fn ensure_well_formed_payload(
        &self,
        payload: ExecutionData,
    ) -> Result<RecoveredBlock<Self::Block>, NewPayloadError> {
        info!("Evolve engine validator: validating payload");

        // Use inner validator but with custom evolve handling.
        match self.inner.ensure_well_formed_payload(payload.clone()) {
            Ok(sealed_block) => {
                info!("Evolve engine validator: payload validation succeeded");
                sealed_block
                    .try_recover()
                    .map_err(|e| NewPayloadError::Other(e.into()))
            }
            Err(err) => {
                // Log the error for debugging.
                tracing::debug!("Evolve payload validation error: {:?}", err);

                // Check if this is a block hash mismatch error - bypass it for evolve.
                if matches!(err, alloy_rpc_types::engine::PayloadError::BlockHash { .. }) {
                    let block_number = payload.payload.block_number();
                    if self.config.is_hash_rewire_active_for_block(block_number) {
                        tracing::warn!(
                            block_number,
                            "canonical hash rewiring active; rejecting mismatched block hash"
                        );
                        return Err(NewPayloadError::Eth(err));
                    }

                    info!("Evolve engine validator: bypassing block hash mismatch for ev-reth");
                    // For evolve, we trust the payload builder - just parse the block without hash validation.
                    let ExecutionData { payload, sidecar } = payload;
                    let sealed_block = payload.try_into_block_with_sidecar(&sidecar)?.seal_slow();
                    sealed_block
                        .try_recover()
                        .map_err(|e| NewPayloadError::Other(e.into()))
                } else {
                    // For other errors, re-throw them.
                    Err(NewPayloadError::Eth(err))
                }
            }
        }
    }

    fn validate_payload_attributes_against_header(
        &self,
        _attr: &EvolveEnginePayloadAttributes,
        _header: &<Self::Block as reth_primitives_traits::Block>::Header,
    ) -> Result<(), InvalidPayloadAttributesError> {
        // Skip default timestamp validation for evolve.
        Ok(())
    }
}

impl EngineApiValidator<EvolveEngineTypes> for EvolveEngineValidator {
    fn validate_version_specific_fields(
        &self,
        version: EngineApiMessageVersion,
        payload_or_attrs: PayloadOrAttributes<'_, ExecutionData, EvolveEnginePayloadAttributes>,
    ) -> Result<(), EngineObjectValidationError> {
        validate_version_specific_fields(self.chain_spec(), version, payload_or_attrs)
    }

    fn ensure_well_formed_attributes(
        &self,
        version: EngineApiMessageVersion,
        attributes: &EvolveEnginePayloadAttributes,
    ) -> Result<(), EngineObjectValidationError> {
        validate_version_specific_fields(
            self.chain_spec(),
            version,
            PayloadOrAttributes::<ExecutionData, EvolveEnginePayloadAttributes>::PayloadAttributes(
                attributes,
            ),
        )?;

        // Validate evolve-specific attributes.
        if let Some(ref transactions) = attributes.transactions {
            info!(
                "Evolve engine validator: validating {} transactions",
                transactions.len()
            );
        }

        Ok(())
    }
}

/// Evolve engine validator builder.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct EvolveEngineValidatorBuilder;

impl<N> PayloadValidatorBuilder<N> for EvolveEngineValidatorBuilder
where
    N: FullNodeComponents<
        Types: NodeTypes<
            Payload = EvolveEngineTypes,
            ChainSpec = ChainSpec,
            Primitives = reth_ethereum::EthPrimitives,
        >,
    >,
{
    type Validator = EvolveEngineValidator;

    async fn build(self, ctx: &AddOnsContext<'_, N>) -> eyre::Result<Self::Validator> {
        let config = EvolvePayloadBuilderConfig::from_chain_spec(ctx.config.chain.as_ref())?;
        Ok(EvolveEngineValidator::new(ctx.config.chain.clone(), config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::B256;
    use reth_chainspec::ChainSpecBuilder;
    use reth_primitives::{Block, SealedBlock};

    fn validator_with_activation(height: Option<u64>) -> EvolveEngineValidator {
        let chain_spec = Arc::new(ChainSpecBuilder::mainnet().build());
        let mut config = EvolvePayloadBuilderConfig::new();
        config.hash_rewire_activation_height = height;
        EvolveEngineValidator::new(chain_spec, config)
    }

    fn mismatched_payload() -> ExecutionData {
        let sealed_block: SealedBlock<Block> = SealedBlock::default();
        let block_hash = sealed_block.hash();
        let block = sealed_block.into_block();
        let mut data = ExecutionData::from_block_unchecked(block_hash, &block);
        data.payload.as_v1_mut().block_hash = B256::repeat_byte(0x42);
        data
    }

    #[test]
    fn legacy_bypass_allows_mismatch_before_activation() {
        let validator = validator_with_activation(None);
        let payload = mismatched_payload();

        validator
            .ensure_well_formed_payload(payload)
            .expect("hash mismatch should be bypassed before activation");
    }

    #[test]
    fn canonical_mode_rejects_mismatch_after_activation() {
        let validator = validator_with_activation(Some(0));
        let payload = mismatched_payload();

        let result = validator.ensure_well_formed_payload(payload);
        assert!(matches!(
            result,
            Err(NewPayloadError::Eth(
                alloy_rpc_types::engine::PayloadError::BlockHash { .. }
            ))
        ));
    }
}
