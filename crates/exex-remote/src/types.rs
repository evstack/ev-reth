//! Versioned wire types for the remote `ExEx` stream.
//!
//! These types deliberately mirror only the externally relevant execution data.
//! They are not aliases of ev-reth's in-process `Chain<EvPrimitives>` or receipt/transaction
//! structs because the stream needs a stable, consumer-facing schema with explicit semantics.
//! The remote forms can therefore evolve independently while still preserving raw transaction
//! bytes and EV-specific derived metadata.

use alloy_primitives::{Address, Bytes, B256, U256};
use serde::{Deserialize, Serialize};

/// Schema version used by the remote `ExEx` stream.
pub(crate) const REMOTE_EXEX_SCHEMA_VERSION_V1: u32 = 1;

/// Block range metadata used for committed, reverted, and reorg notifications.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteBlockRangeV1 {
    /// Inclusive starting block number.
    pub start_block: u64,
    /// Inclusive ending block number.
    pub end_block: u64,
}

impl RemoteBlockRangeV1 {
    /// Creates a new block range.
    ///
    /// # Panics (debug builds only)
    /// Panics if `start_block > end_block`.
    pub const fn new(start_block: u64, end_block: u64) -> Self {
        debug_assert!(start_block <= end_block, "start block must be <= end block");
        Self {
            start_block,
            end_block,
        }
    }
}

/// Extra block metadata useful to indexers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteBlockMetadataV1 {
    /// Block number.
    pub number: u64,
    /// Block hash.
    pub hash: B256,
    /// Parent block hash.
    pub parent_hash: B256,
    /// Block timestamp.
    pub timestamp: u64,
    /// Block gas limit.
    pub gas_limit: u64,
    /// Gas used by the block.
    pub gas_used: u64,
    /// Fee recipient / beneficiary.
    pub fee_recipient: Address,
    /// Base fee per gas, if the chain uses EIP-1559 style pricing.
    pub base_fee_per_gas: Option<u128>,
}

/// Batch-call metadata carried for `EvNode` transactions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteCallV1 {
    /// Call destination.
    pub to: Option<Address>,
    /// ETH value attached to the call.
    pub value: U256,
    /// Calldata for the call.
    pub input: Bytes,
}

/// Transaction metadata and payload suitable for EV transactions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTransactionV1 {
    /// Transaction hash.
    pub hash: B256,
    /// Sender / recovered executor.
    pub sender: Address,
    /// EIP-2718 transaction type.
    pub tx_type: u8,
    /// Transaction nonce.
    pub nonce: u64,
    /// Gas limit requested by the transaction.
    pub gas_limit: u64,
    /// Legacy-style gas price, if present.
    pub gas_price: Option<u128>,
    /// EIP-1559 max fee per gas.
    pub max_fee_per_gas: u128,
    /// EIP-1559 priority fee cap.
    pub max_priority_fee_per_gas: Option<u128>,
    /// Transaction recipient, if any.
    pub to: Option<Address>,
    /// ETH value transferred by the transaction.
    pub value: U256,
    /// Transaction input.
    pub input: Bytes,
    /// Raw encoded transaction bytes.
    pub raw_2718: Bytes,
    /// Optional recovered sponsor / fee payer.
    pub fee_payer: Option<Address>,
    /// Batch call metadata for `EvNode` transactions.
    pub calls: Vec<RemoteCallV1>,
}

impl RemoteTransactionV1 {
    /// Creates a new transaction payload and validates batch metadata when present.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(
        hash: B256,
        sender: Address,
        tx_type: u8,
        nonce: u64,
        gas_limit: u64,
        gas_price: Option<u128>,
        max_fee_per_gas: u128,
        max_priority_fee_per_gas: Option<u128>,
        to: Option<Address>,
        value: U256,
        input: Bytes,
        raw_2718: Bytes,
        fee_payer: Option<Address>,
        calls: Vec<RemoteCallV1>,
    ) -> Self {
        Self {
            hash,
            sender,
            tx_type,
            nonce,
            gas_limit,
            gas_price,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            to,
            value,
            input,
            raw_2718,
            fee_payer,
            calls,
        }
    }
}

/// Receipt log metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteLogV1 {
    /// Emitting contract.
    pub address: Address,
    /// Indexed topics.
    pub topics: Vec<B256>,
    /// Log data payload.
    pub data: Bytes,
    /// Block-local log index.
    pub log_index: u64,
    /// Transaction-local log index if available.
    pub transaction_log_index: Option<u64>,
}

/// Receipt payload with attached logs and EV metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteReceiptV1 {
    /// Transaction hash associated with the receipt.
    pub transaction_hash: B256,
    /// Receipt status.
    pub status: bool,
    /// Gas used by the transaction.
    pub gas_used: u64,
    /// Cumulative gas used at this point in the block.
    pub cumulative_gas_used: u64,
    /// Contract address created by the transaction, if any.
    pub contract_address: Option<Address>,
    /// Logs emitted by the transaction.
    pub logs: Vec<RemoteLogV1>,
    /// Optional recovered sponsor / fee payer.
    pub fee_payer: Option<Address>,
}

impl RemoteReceiptV1 {
    /// Creates a receipt payload and validates nothing beyond the type boundary.
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(
        transaction_hash: B256,
        status: bool,
        gas_used: u64,
        cumulative_gas_used: u64,
        contract_address: Option<Address>,
        logs: Vec<RemoteLogV1>,
        fee_payer: Option<Address>,
    ) -> Self {
        Self {
            transaction_hash,
            status,
            gas_used,
            cumulative_gas_used,
            contract_address,
            logs,
            fee_payer,
        }
    }
}

/// Block payload with transactions, receipts, and metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteBlockV1 {
    /// Block metadata.
    pub metadata: RemoteBlockMetadataV1,
    /// Transactions in execution order.
    pub transactions: Vec<RemoteTransactionV1>,
    /// Receipts in execution order.
    pub receipts: Vec<RemoteReceiptV1>,
}

impl RemoteBlockV1 {
    /// Creates a block payload.
    ///
    /// # Panics (debug builds only)
    /// Panics if `transactions.len() != receipts.len()`.
    pub fn new(
        metadata: RemoteBlockMetadataV1,
        transactions: Vec<RemoteTransactionV1>,
        receipts: Vec<RemoteReceiptV1>,
    ) -> Self {
        debug_assert_eq!(
            transactions.len(),
            receipts.len(),
            "transactions and receipts must have matching lengths"
        );
        Self {
            metadata,
            transactions,
            receipts,
        }
    }
}

/// Remote notification variants carried over the transport.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteNotificationV1 {
    /// Canonical chain extension.
    ChainCommitted {
        /// Inclusive committed block range.
        range: RemoteBlockRangeV1,
        /// Newly committed blocks.
        blocks: Vec<RemoteBlockV1>,
    },
    /// Chain reorganization.
    ChainReorged {
        /// Reverted block range.
        reverted: RemoteBlockRangeV1,
        /// Reverted blocks from the old branch.
        reverted_blocks: Vec<RemoteBlockV1>,
        /// Committed block range for the replacement branch.
        committed: RemoteBlockRangeV1,
        /// Newly committed replacement blocks.
        committed_blocks: Vec<RemoteBlockV1>,
    },
    /// Explicit revert notification.
    ChainReverted {
        /// Inclusive reverted block range.
        reverted: RemoteBlockRangeV1,
        /// Reverted blocks from the old branch.
        reverted_blocks: Vec<RemoteBlockV1>,
    },
}

impl RemoteNotificationV1 {
    /// Returns the schema version associated with the wire format.
    pub const fn schema_version() -> u32 {
        REMOTE_EXEX_SCHEMA_VERSION_V1
    }
}
