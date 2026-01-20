//! Deploy allowlist settings for contract creation control.

use alloy_primitives::Address;
use std::sync::Arc;

/// Settings for gating contract deployment by caller allowlist.
#[derive(Debug, Clone)]
pub struct DeployAllowlistSettings {
    allowlist: Arc<[Address]>,
    activation_height: u64,
}

impl DeployAllowlistSettings {
    /// Creates a new deploy allowlist configuration.
    /// An empty allowlist disables gating and allows all callers.
    pub fn new(allowlist: Vec<Address>, activation_height: u64) -> Self {
        let mut allowlist = allowlist;
        allowlist.sort_unstable();
        Self {
            allowlist: Arc::from(allowlist),
            activation_height,
        }
    }

    /// Returns the activation height for deploy allowlist enforcement.
    pub const fn activation_height(&self) -> u64 {
        self.activation_height
    }

    /// Returns the allowlisted caller addresses.
    pub fn allowlist(&self) -> &[Address] {
        &self.allowlist
    }

    /// Returns true if the allowlist is active at the given block number.
    pub const fn is_active(&self, block_number: u64) -> bool {
        block_number >= self.activation_height
    }

    /// Returns true if the caller is in the allowlist.
    pub fn is_allowed(&self, caller: Address) -> bool {
        if self.allowlist.is_empty() {
            return true;
        }
        self.allowlist.binary_search(&caller).is_ok()
    }
}
