//! Bytecode patching for Solidity immutable variables.
//!
//! Solidity `immutable` values are embedded in the **runtime bytecode** by the
//! compiler, not in storage.  When compiling with placeholder values (e.g.
//! `address(0)`, `uint32(0)`), the compiler leaves zero-filled regions at known
//! byte offsets.  This module replaces those regions with the actual values from
//! the deploy config at genesis-generation time.

use alloy_primitives::{B256, U256};

/// A single immutable reference inside a bytecode blob.
#[derive(Debug, Clone, Copy)]
pub struct ImmutableRef {
    /// Byte offset into the **runtime** bytecode.
    pub start: usize,
    /// Number of bytes (always 32 for EVM words).
    pub length: usize,
}

/// Patch a mutable bytecode slice, writing `value` at every listed offset.
///
/// # Panics
///
/// Panics if any reference extends past the end of `bytecode`.
pub fn patch_bytes(bytecode: &mut [u8], refs: &[ImmutableRef], value: &[u8; 32]) {
    for r in refs {
        assert!(
            r.start + r.length <= bytecode.len(),
            "immutable ref out of bounds: start={} length={} bytecode_len={}",
            r.start,
            r.length,
            bytecode.len()
        );
        bytecode[r.start..r.start + r.length].copy_from_slice(value);
    }
}

/// Convenience: patch with an ABI-encoded `uint256`.
pub fn patch_u256(bytecode: &mut [u8], refs: &[ImmutableRef], val: U256) {
    let word = B256::from(val);
    patch_bytes(bytecode, refs, &word.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_single_ref() {
        let mut bytecode = vec![0u8; 64];
        let refs = [ImmutableRef {
            start: 10,
            length: 32,
        }];
        let value = B256::from(U256::from(42u64));
        patch_bytes(&mut bytecode, &refs, &value.0);

        assert_eq!(bytecode[41], 42);
        // bytes before are untouched
        assert_eq!(bytecode[9], 0);
        // bytes after are untouched
        assert_eq!(bytecode[42], 0);
    }

    #[test]
    #[should_panic(expected = "immutable ref out of bounds")]
    fn patch_out_of_bounds_panics() {
        let mut bytecode = vec![0u8; 16];
        let refs = [ImmutableRef {
            start: 0,
            length: 32,
        }];
        let value = [0u8; 32];
        patch_bytes(&mut bytecode, &refs, &value);
    }
}
