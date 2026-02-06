//! Pooled transaction envelope for ev-reth.
//!
//! This module defines [`EvPooledTxEnvelope`], the transaction type used by Reth's transaction
//! pool. It wraps both standard Ethereum pooled transactions (which may include blob sidecars)
//! and EvNode transactions.
//!
//! The traits implemented here are required by Reth's transaction pool infrastructure:
//! - [`InMemorySize`]: Memory accounting for pool size limits
//! - [`SignerRecoverable`]: Sender address recovery for validation
//! - [`TxHashRef`]: Transaction hash access for deduplication
//! - [`SignedTransaction`]: Marker trait for signed transaction types

use alloy_consensus::{
    error::ValueError,
    transaction::{SignerRecoverable, TxHashRef},
    TransactionEnvelope,
};
use alloy_primitives::{Address, B256};
use reth_primitives_traits::{InMemorySize, SignedTransaction};

use crate::tx::{EvNodeSignedTx, EvTxEnvelope};

/// Pooled transaction envelope with optional blob sidecar support.
#[derive(Clone, Debug, TransactionEnvelope)]
#[envelope(tx_type_name = EvPooledTxType)]
pub enum EvPooledTxEnvelope {
    /// Standard Ethereum pooled transaction envelope (may include blob sidecar).
    #[envelope(flatten)]
    Ethereum(reth_ethereum_primitives::PooledTransactionVariant),
    /// EvNode typed transaction (no sidecar).
    #[envelope(ty = 0x76)]
    EvNode(EvNodeSignedTx),
}

impl InMemorySize for EvPooledTxEnvelope {
    fn size(&self) -> usize {
        match self {
            EvPooledTxEnvelope::Ethereum(tx) => tx.size(),
            EvPooledTxEnvelope::EvNode(tx) => tx.size(),
        }
    }
}

impl SignerRecoverable for EvPooledTxEnvelope {
    fn recover_signer(&self) -> Result<Address, alloy_consensus::crypto::RecoveryError> {
        match self {
            EvPooledTxEnvelope::Ethereum(tx) => tx.recover_signer(),
            EvPooledTxEnvelope::EvNode(tx) => tx
                .signature()
                .recover_address_from_prehash(&tx.tx().executor_signing_hash())
                .map_err(|_| alloy_consensus::crypto::RecoveryError::new()),
        }
    }

    fn recover_signer_unchecked(&self) -> Result<Address, alloy_consensus::crypto::RecoveryError> {
        self.recover_signer()
    }
}

impl TxHashRef for EvPooledTxEnvelope {
    fn tx_hash(&self) -> &B256 {
        match self {
            EvPooledTxEnvelope::Ethereum(tx) => tx.tx_hash(),
            EvPooledTxEnvelope::EvNode(tx) => tx.hash(),
        }
    }
}

impl TryFrom<EvTxEnvelope> for EvPooledTxEnvelope {
    type Error = ValueError<reth_ethereum_primitives::TransactionSigned>;

    fn try_from(value: EvTxEnvelope) -> Result<Self, Self::Error> {
        match value {
            EvTxEnvelope::Ethereum(tx) => Ok(Self::Ethereum(tx.try_into()?)),
            EvTxEnvelope::EvNode(tx) => Ok(Self::EvNode(tx)),
        }
    }
}

impl From<EvPooledTxEnvelope> for EvTxEnvelope {
    fn from(value: EvPooledTxEnvelope) -> Self {
        match value {
            EvPooledTxEnvelope::Ethereum(tx) => EvTxEnvelope::Ethereum(tx.into()),
            EvPooledTxEnvelope::EvNode(tx) => EvTxEnvelope::EvNode(tx),
        }
    }
}

impl SignedTransaction for EvPooledTxEnvelope {}
