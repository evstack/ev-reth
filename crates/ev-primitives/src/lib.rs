//! EV-specific primitive types, including the EvNode 0x76 transaction.

mod pool;
mod tx;

pub use pool::{EvPooledTxEnvelope, EvPooledTxType};
pub use tx::{
    Call, EvNodeSignedTx, EvNodeTransaction, EvTxEnvelope, EvTxType, TransactionSigned,
    EVNODE_SPONSOR_DOMAIN, EVNODE_TX_TYPE_ID,
};

use reth_primitives_traits::NodePrimitives;

/// Block type alias for ev-reth.
pub type Block = alloy_consensus::Block<TransactionSigned>;

/// Block body type alias for ev-reth.
pub type BlockBody = alloy_consensus::BlockBody<TransactionSigned>;

/// Receipt type alias for ev-reth.
pub type Receipt = reth_ethereum_primitives::Receipt<EvTxType>;

/// Helper struct that specifies the ev-reth NodePrimitives types.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvPrimitives;

impl NodePrimitives for EvPrimitives {
    type Block = Block;
    type BlockHeader = alloy_consensus::Header;
    type BlockBody = BlockBody;
    type SignedTx = TransactionSigned;
    type Receipt = Receipt;
}
