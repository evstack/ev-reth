use alloy_eips::{eip4895::Withdrawals, Decodable2718};
use alloy_primitives::{Address, Bytes, B256};
use alloy_rpc_types::{
    engine::{PayloadAttributes as EthPayloadAttributes, PayloadId},
    Withdrawal,
};
use reth_ethereum::{
    node::api::payload::{PayloadAttributes, PayloadBuilderAttributes},
    TransactionSigned,
};
use reth_payload_builder::EthPayloadBuilderAttributes;
use serde::{Deserialize, Serialize};

use crate::error::RollkitEngineError;

/// Rollkit payload attributes that support passing transactions via Engine API
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollkitEnginePayloadAttributes {
    /// Standard Ethereum payload attributes
    #[serde(flatten)]
    pub inner: EthPayloadAttributes,
    /// Transactions to be included in the payload (passed via Engine API)
    pub transactions: Option<Vec<Bytes>>,
    /// Optional gas limit for the payload
    #[serde(rename = "gasLimit")]
    pub gas_limit: Option<u64>,
}

impl PayloadAttributes for RollkitEnginePayloadAttributes {
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

/// Rollkit payload builder attributes
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RollkitEnginePayloadBuilderAttributes {
    /// Ethereum payload builder attributes
    pub ethereum_attributes: EthPayloadBuilderAttributes,
    /// Decoded transactions from the Engine API
    pub transactions: Vec<TransactionSigned>,
    /// Gas limit for the payload
    pub gas_limit: Option<u64>,
}

impl PayloadBuilderAttributes for RollkitEnginePayloadBuilderAttributes {
    type RpcPayloadAttributes = RollkitEnginePayloadAttributes;
    type Error = RollkitEngineError;

    fn try_new(
        parent: B256,
        attributes: RollkitEnginePayloadAttributes,
        _version: u8,
    ) -> Result<Self, Self::Error> {
        let ethereum_attributes = EthPayloadBuilderAttributes::new(parent, attributes.inner);

        // Decode transactions from bytes if provided
        let transactions = attributes
            .transactions
            .unwrap_or_default()
            .into_iter()
            .map(|tx_bytes| {
                TransactionSigned::network_decode(&mut tx_bytes.as_ref())
                    .map_err(|e| RollkitEngineError::InvalidTransactionData(e.to_string()))
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
