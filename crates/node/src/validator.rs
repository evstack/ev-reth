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
///
/// This validator extends the standard Ethereum payload validation with support for
/// legacy block hash compatibility. See [`EvolvePayloadBuilderConfig::canonical_hash_activation_height`]
/// for details on the migration strategy.
///
/// # Block Hash Validation
///
/// Early versions of ev-node passed block hashes from height H-1 instead of H,
/// causing block explorers (e.g., Blockscout) to show all blocks as forks. This
/// validator can bypass hash mismatch errors for historical blocks while enforcing
/// canonical validation for new blocks, controlled by `canonical_hash_activation_height`.
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

        match self.inner.ensure_well_formed_payload(payload.clone()) {
            Ok(sealed_block) => {
                info!("Evolve engine validator: payload validation succeeded");
                sealed_block
                    .try_recover()
                    .map_err(|e| NewPayloadError::Other(e.into()))
            }
            Err(err) => {
                tracing::debug!("Evolve payload validation error: {:?}", err);

                // Handle block hash mismatch errors specially for legacy compatibility.
                //
                // Background: Early versions of ev-node passed block hashes from height H-1
                // instead of H when communicating with ev-reth via the Engine API. This caused
                // block hashes to not match the canonical Ethereum block hash (keccak256 of
                // RLP-encoded header), resulting in block explorers like Blockscout incorrectly
                // displaying every block as a fork due to parent hash mismatches.
                //
                // For existing networks with historical blocks containing these non-canonical
                // hashes, we need to bypass this validation to allow nodes to sync from genesis.
                // The `canonical_hash_activation_height` config controls when to start enforcing
                // canonical hashes for new blocks.
                if matches!(err, alloy_rpc_types::engine::PayloadError::BlockHash { .. }) {
                    let block_number = payload.payload.block_number();

                    // If canonical hash enforcement is active for this block, reject the mismatch
                    if self.config.is_canonical_hash_enforced(block_number) {
                        tracing::warn!(
                            block_number,
                            "canonical hash enforcement active; rejecting mismatched block hash"
                        );
                        return Err(NewPayloadError::Eth(err));
                    }

                    // Legacy mode: bypass hash mismatch to allow syncing historical blocks.
                    // Re-seal the block with the correct canonical hash (keccak256 of header).
                    info!(
                        block_number,
                        "bypassing block hash mismatch (legacy mode before activation height)"
                    );
                    let ExecutionData { payload, sidecar } = payload;
                    let sealed_block = payload.try_into_block_with_sidecar(&sidecar)?.seal_slow();
                    sealed_block
                        .try_recover()
                        .map_err(|e| NewPayloadError::Other(e.into()))
                } else {
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

    fn validator_with_activation(activation_height: Option<u64>) -> EvolveEngineValidator {
        let chain_spec = Arc::new(ChainSpecBuilder::mainnet().build());
        let mut config = EvolvePayloadBuilderConfig::new();
        config.canonical_hash_activation_height = activation_height;
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
    fn test_legacy_mode_bypasses_hash_mismatch() {
        // When activation height is set in the future, legacy mode should bypass hash mismatches
        let validator = validator_with_activation(Some(1000));
        let payload = mismatched_payload();

        validator
            .ensure_well_formed_payload(payload)
            .expect("hash mismatch should be bypassed in legacy mode");
    }

    #[test]
    fn test_canonical_mode_rejects_hash_mismatch() {
        // When activation height is 0 (or in the past), canonical mode should reject mismatches
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

    #[test]
    fn test_default_enforces_canonical_hash() {
        // When no activation height is set, canonical validation should be enforced (default)
        let validator = validator_with_activation(None);
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
