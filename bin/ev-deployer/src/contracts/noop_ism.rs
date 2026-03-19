//! `NoopIsm` bytecode encoding.
//!
//! `NoopIsm` is a Hyperlane Interchain Security Module (ISM) that accepts all
//! messages without verification — `verify` always returns `true`.
//!
//! ## Immutables
//!
//! None.
//!
//! ## Storage layout
//!
//! None.

use crate::{config::NoopIsmConfig, contracts::GenesisContract};
use alloy_primitives::{hex, Bytes};
use std::collections::BTreeMap;

/// `NoopIsm` runtime bytecode compiled with Hyperlane v11.0.3,
/// solc 0.8.22 (Foundry `ci` profile: `cbor_metadata=false`, `bytecode_hash="none"`).
///
/// Regenerate with:
/// ```sh
/// cd contracts/lib/hyperlane-monorepo/solidity && \
///   forge soldeer install && \
///   FOUNDRY_PROFILE=ci forge inspect NoopIsm deployedBytecode
/// ```
const NOOP_ISM_BYTECODE: &[u8] = &hex!("608060405234801561001057600080fd5b50600436106100415760003560e01c80636465e69f1461004657806393c4484714610065578063f7e83aee146100ae575b600080fd5b61004e600681565b60405160ff90911681526020015b60405180910390f35b6100a16040518060400160405280600681526020017f31312e302e33000000000000000000000000000000000000000000000000000081525081565b60405161005c91906100d6565b6100c66100bc36600461018c565b6001949350505050565b604051901515815260200161005c565b60006020808352835180602085015260005b81811015610104578581018301518582016040015282016100e8565b5060006040828601015260407fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0601f8301168501019250505092915050565b60008083601f84011261015557600080fd5b50813567ffffffffffffffff81111561016d57600080fd5b60208301915083602082850101111561018557600080fd5b9250929050565b600080600080604085870312156101a257600080fd5b843567ffffffffffffffff808211156101ba57600080fd5b6101c688838901610143565b909650945060208701359150808211156101df57600080fd5b506101ec87828801610143565b9598949750955050505056");

/// Build a genesis alloc entry for `NoopIsm`.
pub(crate) fn build(config: &NoopIsmConfig) -> GenesisContract {
    GenesisContract {
        address: config.address,
        code: Bytes::from(NOOP_ISM_BYTECODE.to_vec()),
        storage: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{address, hex};
    use std::{path::PathBuf, process::Command};

    fn test_config() -> NoopIsmConfig {
        NoopIsmConfig {
            address: address!("0000000000000000000000000000000000001300"),
        }
    }

    #[test]
    fn storage_is_empty() {
        let contract = build(&test_config());
        assert!(
            contract.storage.is_empty(),
            "NoopIsm should have no storage"
        );
    }

    #[test]
    fn bytecode_is_present() {
        let contract = build(&test_config());
        assert!(
            !contract.code.is_empty(),
            "NoopIsm should have non-empty bytecode"
        );
    }

    #[test]
    #[ignore = "requires forge CLI"]
    fn noop_ism_bytecode_matches_solidity_source() {
        let contracts_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .join("contracts")
            .join("lib")
            .join("hyperlane-monorepo")
            .join("solidity");

        let output = Command::new("forge")
            .args(["inspect", "NoopIsm", "deployedBytecode", "--root"])
            .arg(&contracts_root)
            .env("FOUNDRY_PROFILE", "ci")
            .output()
            .expect("forge not found");

        assert!(
            output.status.success(),
            "forge inspect failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let forge_hex = String::from_utf8(output.stdout)
            .unwrap()
            .trim()
            .strip_prefix("0x")
            .unwrap()
            .to_lowercase();

        let hardcoded_hex = hex::encode(NOOP_ISM_BYTECODE);

        assert_eq!(
            forge_hex, hardcoded_hex,
            "NoopIsm bytecode mismatch! Regenerate with: \
             cd contracts/lib/hyperlane-monorepo/solidity && \
             FOUNDRY_PROFILE=ci forge inspect NoopIsm deployedBytecode"
        );
    }
}
