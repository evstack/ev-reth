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
use tracing::{debug, info, instrument};

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

    fn convert_payload_to_block(
        &self,
        payload: ExecutionData,
    ) -> Result<SealedBlock<Self::Block>, NewPayloadError> {
        self.inner
            .ensure_well_formed_payload(payload)
            .map_err(NewPayloadError::other)
    }

    #[instrument(skip(self, payload), fields(
        block_number = payload.payload.block_number(),
        tx_count = payload.payload.transactions().len(),
        block_hash = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
    ))]
    fn ensure_well_formed_payload(
        &self,
        payload: ExecutionData,
    ) -> Result<RecoveredBlock<Self::Block>, NewPayloadError> {
        let _start = std::time::Instant::now();
        // Use inner validator but with custom evolve handling.
        match self.inner.ensure_well_formed_payload(payload.clone()) {
            Ok(sealed_block) => {
                let span = tracing::Span::current();
                span.record("block_hash", tracing::field::display(sealed_block.hash()));
                span.record("duration_ms", _start.elapsed().as_millis() as u64);
                info!("payload validation succeeded");
                let ev_block = convert_sealed_block(sealed_block);
                ev_block
                    .try_recover()
                    .map_err(|e| NewPayloadError::Other(e.into()))
            }
            Err(err) => {
                debug!(error = ?err, "payload validation error");

                // Check if this is an error we can bypass for evolve:
                // 1. BlockHash mismatch - ev-reth computes different hash due to custom tx types
                // 2. Unknown tx type (0x76) - standard decoder doesn't recognize EvNode transactions
                //
                // Note: The tx type error comes as PayloadError::Decode(alloy_rlp::Error::Custom(...))
                // Since it's a Custom error with a string, we must use string matching for the
                // specific message. This is fragile - if alloy changes the error message, this
                // bypass will silently break. The test `decode_error_contains_expected_message`
                // in this module helps catch such regressions.
                let should_bypass =
                    matches!(err, alloy_rpc_types::engine::PayloadError::BlockHash { .. })
                        || is_unknown_tx_type_error(&err);

                if should_bypass {
                    info!(error = ?err, "bypassing validation error for ev-reth");
                    // For evolve, we trust the payload builder - parse the block with EvNode support.
                    let ev_block = parse_evolve_payload(payload)?;
                    let span = tracing::Span::current();
                    span.record("block_hash", tracing::field::display(ev_block.hash()));
                    span.record("duration_ms", _start.elapsed().as_millis() as u64);
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

/// The error message fragment used by alloy when encountering an unknown transaction type.
/// This is used for string matching since the error is a generic `Custom` error.
const UNKNOWN_TX_TYPE_ERROR_MSG: &str = "unexpected tx type";

/// Checks if a `PayloadError` indicates an unknown transaction type (e.g., `EvNode`'s 0x76).
///
/// This uses string matching because alloy returns `alloy_rlp::Error::Custom("unexpected tx type")`
/// which doesn't have a dedicated error variant. The constant `UNKNOWN_TX_TYPE_ERROR_MSG` is
/// tested in `decode_error_contains_expected_message` to catch upstream changes.
fn is_unknown_tx_type_error(err: &alloy_rpc_types::engine::PayloadError) -> bool {
    matches!(err, alloy_rpc_types::engine::PayloadError::Decode(_))
        && err.to_string().contains(UNKNOWN_TX_TYPE_ERROR_MSG)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_eips::eip2718::Decodable2718;
    use alloy_rlp::Error as RlpError;

    /// Verifies that the error message for unknown tx types matches our expected string.
    /// This test will fail if alloy changes the error message, alerting us to update
    /// `UNKNOWN_TX_TYPE_ERROR_MSG` and the bypass logic.
    #[test]
    fn decode_error_contains_expected_message() {
        // Try to decode an EvNode transaction (type 0x76) using the standard Ethereum decoder
        // which doesn't recognize this type.
        let evnode_tx_type: u8 = 0x76;
        let fake_tx = vec![evnode_tx_type, 0xc0]; // Type byte + minimal RLP

        let result =
            reth_ethereum_primitives::TransactionSigned::decode_2718(&mut fake_tx.as_slice());

        assert!(result.is_err(), "Decoding unknown tx type should fail");

        let err = result.unwrap_err();
        let err_string = err.to_string();

        assert!(
            err_string.contains(UNKNOWN_TX_TYPE_ERROR_MSG),
            "Error message should contain '{}', but got: '{}'",
            UNKNOWN_TX_TYPE_ERROR_MSG,
            err_string
        );
    }

    #[test]
    fn ensure_well_formed_payload_span_has_expected_fields() {
        use crate::test_utils::SpanCollector;
        use alloy_primitives::{Address, Bloom, Bytes, B256, U256};
        use alloy_rpc_types::engine::{
            ExecutionData, ExecutionPayload, ExecutionPayloadSidecar, ExecutionPayloadV1,
            ExecutionPayloadV2, ExecutionPayloadV3,
        };
        use reth_chainspec::ChainSpecBuilder;

        let collector = SpanCollector::new();
        let _guard = collector.as_default();

        let chain_spec = std::sync::Arc::new(
            ChainSpecBuilder::default()
                .chain(reth_chainspec::Chain::from_id(1234))
                .genesis(
                    serde_json::from_str(include_str!("../../tests/assets/genesis.json"))
                        .expect("valid genesis"),
                )
                .cancun_activated()
                .build(),
        );
        let validator = EvolveEngineValidator::new(chain_spec);

        let v1 = ExecutionPayloadV1 {
            parent_hash: B256::ZERO,
            fee_recipient: Address::ZERO,
            state_root: B256::ZERO,
            receipts_root: B256::ZERO,
            logs_bloom: Bloom::ZERO,
            prev_randao: B256::ZERO,
            block_number: 1,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp: 1710338136,
            extra_data: Bytes::default(),
            base_fee_per_gas: U256::ZERO,
            block_hash: B256::ZERO,
            transactions: vec![],
        };
        let v2 = ExecutionPayloadV2 {
            payload_inner: v1,
            withdrawals: vec![],
        };
        let v3 = ExecutionPayloadV3 {
            payload_inner: v2,
            blob_gas_used: 0,
            excess_blob_gas: 0,
        };

        let payload = ExecutionPayload::V3(v3);
        let sidecar = ExecutionPayloadSidecar::default();
        let execution_data = ExecutionData::new(payload, sidecar);

        // we only care that the span was created, not whether validation succeeds.
        let _ = PayloadValidator::ensure_well_formed_payload(&validator, execution_data);

        let span = collector
            .find_span("ensure_well_formed_payload")
            .expect("ensure_well_formed_payload span should be recorded");

        assert!(span.has_field("block_number"), "span missing block_number field");
        assert!(span.has_field("tx_count"), "span missing tx_count field");
        assert!(span.has_field("block_hash"), "span missing block_hash field");
        assert!(span.has_field("duration_ms"), "span missing duration_ms field");
    }

    /// Verifies that `is_unknown_tx_type_error` correctly identifies decode errors
    /// with the expected message.
    #[test]
    fn is_unknown_tx_type_error_matches_decode_errors() {
        use alloy_rpc_types::engine::PayloadError;

        // Create a Decode error with the expected message
        let decode_err = PayloadError::Decode(RlpError::Custom(UNKNOWN_TX_TYPE_ERROR_MSG));
        assert!(
            is_unknown_tx_type_error(&decode_err),
            "Should match Decode error with expected message"
        );

        // Create a Decode error with a different message
        let other_decode_err = PayloadError::Decode(RlpError::Custom("some other error"));
        assert!(
            !is_unknown_tx_type_error(&other_decode_err),
            "Should not match Decode error with different message"
        );

        // BlockHash error should not match
        let block_hash_err = PayloadError::BlockHash {
            execution: alloy_primitives::B256::ZERO,
            consensus: alloy_primitives::B256::ZERO,
        };
        assert!(
            !is_unknown_tx_type_error(&block_hash_err),
            "Should not match BlockHash error"
        );
    }
}
