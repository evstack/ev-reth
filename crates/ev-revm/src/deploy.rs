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

/// Error returned by deploy allowlist checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployCheckError {
    /// Caller is not allowed to perform top-level contract creation.
    NotAllowed,
}

// Intentionally no envelope discriminator here to keep dependencies light.

/// Enforces the deploy allowlist policy.
///
/// If `is_top_level_create` is false or settings are None or not active yet, this is a no-op.
/// Otherwise returns `NotAllowed` if `caller` is not in the allowlist.
pub fn check_deploy_allowed(
    settings: Option<&DeployAllowlistSettings>,
    caller: Address,
    is_top_level_create: bool,
    block_number: u64,
) -> Result<(), DeployCheckError> {
    if !is_top_level_create {
        return Ok(());
    }
    let Some(settings) = settings else {
        return Ok(());
    };
    if !settings.is_active(block_number) {
        return Ok(());
    }
    if settings.is_allowed(caller) {
        Ok(())
    } else {
        Err(DeployCheckError::NotAllowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn empty_allowlist_allows_any_caller() {
        let settings = DeployAllowlistSettings::new(vec![], 0);
        let caller = address!("0x00000000000000000000000000000000000000aa");
        assert!(settings.is_allowed(caller));
    }

    #[test]
    fn check_deploy_allowed_with_empty_settings_allows() {
        let settings = DeployAllowlistSettings::new(vec![], 0);
        let caller = address!("0x00000000000000000000000000000000000000bb");
        let result = check_deploy_allowed(Some(&settings), caller, true, 0);
        assert!(result.is_ok());
    }
}
