use alloy_consensus::BlockHeader;
use alloy_eips::{eip4895::Withdrawals, Decodable2718};
use alloy_primitives::{Address, Bytes, B256};
use alloy_rpc_types::{
    engine::{PayloadAttributes as RpcPayloadAttributes, PayloadId},
    Withdrawal,
};
use reth_chainspec::EthereumHardforks;
use reth_engine_local::payload::LocalPayloadAttributesBuilder;
use reth_ethereum::node::api::payload::{PayloadAttributes, PayloadBuilderAttributes};
use reth_payload_builder::EthPayloadBuilderAttributes;
use reth_payload_primitives::PayloadAttributesBuilder;
use reth_primitives_traits::SealedHeader;
use serde::{Deserialize, Serialize};

use crate::error::EvolveEngineError;
use ev_primitives::TransactionSigned;

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvolveEnginePayloadBuilderAttributes {
    /// Ethereum payload builder attributes.
    pub ethereum_attributes: EthPayloadBuilderAttributes,
    /// Decoded transactions from the Engine API.
    pub transactions: Vec<TransactionSigned>,
    /// Gas limit for the payload.
    pub gas_limit: Option<u64>,
}

impl PayloadBuilderAttributes for EvolveEnginePayloadBuilderAttributes {
    type RpcPayloadAttributes = EvolveEnginePayloadAttributes;
    type Error = EvolveEngineError;

    fn try_new(
        parent: B256,
        attributes: EvolveEnginePayloadAttributes,
        _version: u8,
    ) -> Result<Self, Self::Error> {
        let ethereum_attributes = EthPayloadBuilderAttributes::new(parent, attributes.inner);

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

        Ok(Self {
            ethereum_attributes,
            transactions,
            gas_limit: attributes.gas_limit,
        })
    }

    fn payload_id(&self) -> PayloadId {
        self.ethereum_attributes.id
    }

    fn parent(&self) -> B256 {
        self.ethereum_attributes.parent
    }

    fn timestamp(&self) -> u64 {
        self.ethereum_attributes.timestamp
    }

    fn parent_beacon_block_root(&self) -> Option<B256> {
        self.ethereum_attributes.parent_beacon_block_root
    }

    fn suggested_fee_recipient(&self) -> Address {
        self.ethereum_attributes.suggested_fee_recipient
    }

    fn prev_randao(&self) -> B256 {
        self.ethereum_attributes.prev_randao
    }

    fn withdrawals(&self) -> &Withdrawals {
        &self.ethereum_attributes.withdrawals
    }
}

impl From<EthPayloadBuilderAttributes> for EvolveEnginePayloadBuilderAttributes {
    fn from(eth: EthPayloadBuilderAttributes) -> Self {
        Self {
            ethereum_attributes: eth,
            transactions: Vec::new(),
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
