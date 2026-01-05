//! Common utilities and constants for ev-reth

pub mod constants;
pub mod sponsor_transaction;

pub use constants::*;
pub use sponsor_transaction::{SponsorTransaction, FEE_PAYER_SIGNATURE_MAGIC_BYTE, SPONSOR_TX_TYPE_ID};
