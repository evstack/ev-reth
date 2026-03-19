//! Bytecode patching for Solidity immutable variables.
//!
//! Solidity `immutable` values are embedded in the **runtime bytecode** by the
//! compiler, not in storage.  When compiling with placeholder values (e.g.
//! `address(0)`, `uint32(0)`), the compiler leaves zero-filled regions at known
//! byte offsets.  This module replaces those regions with the actual values from
//! the deploy config at genesis-generation time.

use alloy_primitives::{Address, B256, U256};

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

/// Convenience: patch with an ABI-encoded `address` (left-padded to 32 bytes).
pub fn patch_address(bytecode: &mut [u8], refs: &[ImmutableRef], addr: Address) {
    let word: B256 = B256::from(U256::from_be_bytes(addr.into_word().0));
    patch_bytes(bytecode, refs, &word.0);
}

/// Convenience: patch with an ABI-encoded `uint32` (left-padded to 32 bytes).
pub fn patch_u32(bytecode: &mut [u8], refs: &[ImmutableRef], val: u32) {
    let word = B256::from(U256::from(val));
    patch_bytes(bytecode, refs, &word.0);
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
    fn patch_multiple_refs() {
        let mut bytecode = vec![0u8; 128];
        let refs = [
            ImmutableRef {
                start: 0,
                length: 32,
            },
            ImmutableRef {
                start: 64,
                length: 32,
            },
        ];
        let addr = Address::repeat_byte(0xAB);
        patch_address(&mut bytecode, &refs, addr);

        // Both locations should have the address (last 20 bytes of the 32-byte word)
        assert_eq!(bytecode[12..32], [0xAB; 20]);
        assert_eq!(bytecode[76..96], [0xAB; 20]);
        // Padding bytes should be zero
        assert_eq!(bytecode[0..12], [0u8; 12]);
        assert_eq!(bytecode[64..76], [0u8; 12]);
    }

    #[test]
    fn patch_u32_value() {
        let mut bytecode = vec![0u8; 64];
        let refs = [ImmutableRef {
            start: 0,
            length: 32,
        }];
        patch_u32(&mut bytecode, &refs, 1234);

        // uint32 1234 = 0x04D2, left-padded to 32 bytes
        assert_eq!(bytecode[30], 0x04);
        assert_eq!(bytecode[31], 0xD2);
        assert_eq!(bytecode[0..30], [0u8; 30]);
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
