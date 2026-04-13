use alloy_consensus::BlockHeader;
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
