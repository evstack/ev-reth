#![allow(missing_docs, rustdoc::missing_crate_level_docs)]

use std::sync::Arc;

use alloy_consensus::Header;
use alloy_eips::Decodable2718;
use alloy_rpc_types::engine::ExecutionData;
use ev_primitives::{Block as EvBlock, BlockBody as EvBlockBody, EvTxEnvelope};
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
use reth_primitives_traits::{Block as _, RecoveredBlock, SealedBlock};
use tracing::info;

use crate::{attributes::EvolveEnginePayloadAttributes, node::EvolveEngineTypes};

/// Evolve engine validator that handles custom payload validation.
#[derive(Debug, Clone)]
pub struct EvolveEngineValidator {
    inner: EthereumExecutionPayloadValidator<ChainSpec>,
}

impl EvolveEngineValidator {
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

impl PayloadValidator<EvolveEngineTypes> for EvolveEngineValidator {
    type Block = ev_primitives::Block;

    fn ensure_well_formed_payload(
        &self,
        payload: ExecutionData,
    ) -> Result<RecoveredBlock<Self::Block>, NewPayloadError> {
        info!("Evolve engine validator: validating payload");

        // Use inner validator but with custom evolve handling.
        match self.inner.ensure_well_formed_payload(payload.clone()) {
            Ok(sealed_block) => {
                info!("Evolve engine validator: payload validation succeeded");
                let ev_block = convert_sealed_block(sealed_block);
                ev_block
                    .try_recover()
                    .map_err(|e| NewPayloadError::Other(e.into()))
            }
            Err(err) => {
                // Log the error for debugging.
                tracing::debug!("Evolve payload validation error: {:?}", err);

                // Check if this is an error we can bypass for evolve (block hash mismatch,
                // unknown tx type for EvNode transactions).
                let should_bypass =
                    matches!(err, alloy_rpc_types::engine::PayloadError::BlockHash { .. })
                        || err.to_string().contains("unexpected tx type");

                if should_bypass {
                    info!(
                        "Evolve engine validator: bypassing validation error for ev-reth: {:?}",
                        err
                    );
                    // For evolve, we trust the payload builder - parse the block with EvNode support.
                    let ev_block = parse_evolve_payload(payload)?;
                    ev_block
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

fn convert_sealed_block(
    sealed_block: SealedBlock<reth_ethereum::Block>,
) -> SealedBlock<ev_primitives::Block> {
    let (block, hash) = sealed_block.split();
    let ev_block = block.map_transactions(EvTxEnvelope::Ethereum);
    SealedBlock::new_unchecked(ev_block, hash)
}

/// Parses an execution payload containing `EvNode` transactions.
fn parse_evolve_payload(
    payload: ExecutionData,
) -> Result<SealedBlock<ev_primitives::Block>, NewPayloadError> {
    let ExecutionData { payload, sidecar } = payload;

    // Parse transactions using EvTxEnvelope which supports both Ethereum and EvNode types.
    let transactions: Vec<EvTxEnvelope> = payload
        .transactions()
        .iter()
        .map(|tx| {
            EvTxEnvelope::decode_2718(&mut tx.as_ref())
                .map_err(|e| NewPayloadError::Other(Box::new(e)))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Build the block header from payload using the common accessor methods.
    let v1 = payload.as_v1();
    let header = Header {
        parent_hash: payload.parent_hash(),
        ommers_hash: alloy_consensus::EMPTY_OMMER_ROOT_HASH,
        beneficiary: payload.fee_recipient(),
        state_root: v1.state_root,
        transactions_root: alloy_consensus::proofs::calculate_transaction_root(&transactions),
        receipts_root: v1.receipts_root,
        logs_bloom: v1.logs_bloom,
        difficulty: alloy_primitives::U256::ZERO,
        number: payload.block_number(),
        gas_limit: payload.gas_limit(),
        gas_used: v1.gas_used,
        timestamp: payload.timestamp(),
        extra_data: v1.extra_data.clone(),
        mix_hash: payload.prev_randao(),
        nonce: alloy_primitives::B64::ZERO,
        base_fee_per_gas: Some(payload.saturated_base_fee_per_gas()),
        withdrawals_root: payload
            .withdrawals()
            .map(|w| alloy_consensus::proofs::calculate_withdrawals_root(w)),
        blob_gas_used: payload.blob_gas_used(),
        excess_blob_gas: payload.excess_blob_gas(),
        parent_beacon_block_root: sidecar.parent_beacon_block_root(),
        requests_hash: sidecar.requests_hash(),
    };

    // Build block body.
    let body = EvBlockBody {
        transactions,
        ommers: vec![],
        withdrawals: payload.withdrawals().cloned().map(Into::into),
    };

    let block = EvBlock::new(header, body);
    Ok(block.seal_slow())
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
            Primitives = ev_primitives::EvPrimitives,
        >,
    >,
{
    type Validator = EvolveEngineValidator;

    async fn build(self, ctx: &AddOnsContext<'_, N>) -> eyre::Result<Self::Validator> {
        Ok(EvolveEngineValidator::new(ctx.config.chain.clone()))
    }
}
