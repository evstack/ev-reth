//! Contract bytecode and storage encoding.

pub mod admin_proxy;
pub mod fee_vault;
pub mod immutables;
pub mod mailbox;
pub mod merkle_tree_hook;
pub mod noop_ism;
pub mod permit2;
pub mod protocol_fee;

use alloy_primitives::{Address, Bytes, B256};
use std::collections::BTreeMap;

/// A contract ready to be placed in genesis alloc.
#[derive(Debug)]
pub struct GenesisContract {
    /// The address to deploy at.
    pub address: Address,
    /// Runtime bytecode.
    pub code: Bytes,
    /// Storage slot values.
    pub storage: BTreeMap<B256, B256>,
}
