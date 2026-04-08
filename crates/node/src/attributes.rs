use alloy_consensus::BlockHeader;
use alloy_eips::Decodable2718;
use alloy_primitives::{Address, Bytes, B256};
use alloy_rpc_types::{
    engine::{PayloadAttributes as RpcPayloadAttributes, PayloadId},
    Withdrawal,
};
use reth_chainspec::EthereumHardforks;
use reth_engine_local::payload::LocalPayloadAttributesBuilder;
use reth_ethereum::node::api::payload::PayloadAttributes;
use reth_payload_primitives::{payload_id, PayloadAttributesBuilder};
use reth_primitives_traits::SealedHeader;
use serde::{Deserialize, Serialize};

use crate::error::EvolveEngineError;
use crate::tracing_ext::RecordDurationOnDrop;
use ev_primitives::TransactionSigned;
use tracing::{info, instrument};

/// Evolve payload attributes that support passing transactions via Engine API.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolveEnginePayloadAttributes {
    /// Standard Ethereum payload attributes.
    #[serde(flatten)]
    pub inner: RpcPayloadAttributes,
    /// Transactions to be included in the payload (passed via Engine API).
    pub transactions: Option<Vec<Bytes>>,
    /// Optional gas limit for the payload.
    #[serde(rename = "gasLimit")]
    pub gas_limit: Option<u64>,
}

impl PayloadAttributes for EvolveEnginePayloadAttributes {
    fn payload_id(&self, parent_hash: &B256) -> PayloadId {
        payload_id(parent_hash, &self.inner)
    }

    fn timestamp(&self) -> u64 {
        self.inner.timestamp()
    }

    fn withdrawals(&self) -> Option<&Vec<Withdrawal>> {
        self.inner.withdrawals()
    }

    fn parent_beacon_block_root(&self) -> Option<B256> {
        self.inner.parent_beacon_block_root()
    }
}

impl From<RpcPayloadAttributes> for EvolveEnginePayloadAttributes {
    fn from(inner: RpcPayloadAttributes) -> Self {
        Self {
            inner,
            transactions: None,
            gas_limit: None,
        }
    }
}

/// Evolve payload builder attributes.
///
/// In reth v2.0.0, `PayloadBuilderAttributes` was removed. This type now implements
/// `PayloadAttributes` directly and stores the decoded transactions from the Engine API.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolveEnginePayloadBuilderAttributes {
    /// The inner RPC payload attributes.
    #[serde(flatten)]
    pub inner: RpcPayloadAttributes,
    /// Parent block hash.
    pub parent: B256,
    /// Decoded transactions from the Engine API.
    #[serde(skip)]
    pub transactions: Vec<TransactionSigned>,
    /// Gas limit for the payload.
    #[serde(rename = "gasLimit")]
    pub gas_limit: Option<u64>,
}

impl EvolveEnginePayloadBuilderAttributes {
    /// Creates builder attributes from RPC attributes with decoded transactions.
    #[instrument(skip(parent, attributes), fields(
        parent_hash = %parent,
        raw_tx_count = attributes.transactions.as_ref().map_or(0, |t| t.len()),
        gas_limit = ?attributes.gas_limit,
        duration_ms = tracing::field::Empty,
    ))]
    pub fn try_new(
        parent: B256,
        attributes: EvolveEnginePayloadAttributes,
    ) -> Result<Self, EvolveEngineError> {
        let _duration = RecordDurationOnDrop::new();

        // Decode transactions from bytes if provided.
        let transactions = attributes
            .transactions
            .unwrap_or_default()
            .into_iter()
            .map(|tx_bytes| {
                TransactionSigned::network_decode(&mut tx_bytes.as_ref())
                    .map_err(|e| EvolveEngineError::InvalidTransactionData(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        info!(
            decoded_tx_count = transactions.len(),
            "decoded payload attributes"
        );

        Ok(Self {
            inner: attributes.inner,
            parent,
            transactions,
            gas_limit: attributes.gas_limit,
        })
    }

    /// Returns the parent block hash.
    pub const fn parent(&self) -> B256 {
        self.parent
    }

    /// Returns the suggested fee recipient.
    pub const fn suggested_fee_recipient(&self) -> Address {
        self.inner.suggested_fee_recipient
    }

    /// Returns the prev randao value.
    pub const fn prev_randao(&self) -> B256 {
        self.inner.prev_randao
    }
}

impl PayloadAttributes for EvolveEnginePayloadBuilderAttributes {
    fn payload_id(&self, parent_hash: &B256) -> PayloadId {
        payload_id(parent_hash, &self.inner)
    }

    fn timestamp(&self) -> u64 {
        self.inner.timestamp
    }

    fn withdrawals(&self) -> Option<&Vec<Withdrawal>> {
        self.inner.withdrawals.as_ref()
    }

    fn parent_beacon_block_root(&self) -> Option<B256> {
        self.inner.parent_beacon_block_root
    }
}

impl From<EvolveEnginePayloadAttributes> for EvolveEnginePayloadBuilderAttributes {
    fn from(attrs: EvolveEnginePayloadAttributes) -> Self {
        Self {
            inner: attrs.inner,
            parent: B256::ZERO,
            transactions: Vec::new(),
            gas_limit: attrs.gas_limit,
        }
    }
}

impl PayloadAttributesBuilder<EvolveEnginePayloadAttributes>
    for LocalPayloadAttributesBuilder<reth_ethereum::chainspec::ChainSpec>
{
    fn build(&self, parent: &SealedHeader) -> EvolveEnginePayloadAttributes {
        // use current time, ensuring it's at least parent + 1
        let timestamp = std::cmp::max(
            parent.timestamp().saturating_add(1),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );

        let inner = RpcPayloadAttributes {
            timestamp,
            prev_randao: B256::random(),
            suggested_fee_recipient: Address::random(),
            withdrawals: self
                .chain_spec
                .is_shanghai_active_at_timestamp(timestamp)
                .then(Default::default),
            parent_beacon_block_root: self
                .chain_spec
                .is_cancun_active_at_timestamp(timestamp)
                .then(B256::random),
        };

        EvolveEnginePayloadAttributes {
            inner,
            transactions: None,
            gas_limit: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::SpanCollector;

    #[test]
    fn try_new_span_has_expected_fields() {
        let collector = SpanCollector::new();
        let _guard = collector.as_default();

        let parent = B256::random();
        let attrs = EvolveEnginePayloadAttributes {
            inner: RpcPayloadAttributes {
                timestamp: 1710338136,
                prev_randao: B256::random(),
                suggested_fee_recipient: Address::random(),
                withdrawals: Some(vec![]),
                parent_beacon_block_root: Some(B256::ZERO),
            },
            transactions: Some(vec![]),
            gas_limit: Some(30_000_000),
        };

        // we only care that the span was created with the right fields.
        let _ = EvolveEnginePayloadBuilderAttributes::try_new(parent, attrs);

        let span = collector
            .find_span("try_new")
            .expect("try_new span should be recorded");

        assert!(
            span.has_field("parent_hash"),
            "span missing parent_hash field"
        );
        assert!(
            span.has_field("raw_tx_count"),
            "span missing raw_tx_count field"
        );
        assert!(span.has_field("gas_limit"), "span missing gas_limit field");
        assert!(
            span.has_field("duration_ms"),
            "span missing duration_ms field"
        );
    }
}
