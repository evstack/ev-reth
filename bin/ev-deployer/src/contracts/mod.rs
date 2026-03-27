//! Contract bytecode and storage encoding.

pub mod admin_proxy;
pub mod immutables;
pub mod permit2;

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
