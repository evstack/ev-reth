//! Contract bytecode and storage encoding.

pub(crate) mod admin_proxy;
pub(crate) mod fee_vault;
pub(crate) mod immutables;
pub(crate) mod mailbox;
pub(crate) mod merkle_tree_hook;
pub(crate) mod noop_ism;
pub(crate) mod protocol_fee;

use alloy_primitives::{Address, Bytes, B256};
use std::collections::BTreeMap;

/// A contract ready to be placed in genesis alloc.
pub(crate) struct GenesisContract {
    /// The address to deploy at.
    pub address: Address,
    /// Runtime bytecode.
    pub code: Bytes,
    /// Storage slot values.
    pub storage: BTreeMap<B256, B256>,
}
