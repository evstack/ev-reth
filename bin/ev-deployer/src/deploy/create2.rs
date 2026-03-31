//! CREATE2 address computation.

use alloy_primitives::{keccak256, Address, Bytes, B256};

/// The deterministic deployer factory address (Nick's factory).
/// See: <https://github.com/Arachnid/deterministic-deployment-proxy>
pub(crate) const DETERMINISTIC_DEPLOYER: Address = Address::new(alloy_primitives::hex!(
    "4e59b44847b379578588920ca78fbf26c0b4956c"
));

/// Compute the CREATE2 address for a contract deployed via the deterministic deployer.
///
/// The factory expects calldata `salt ++ initcode` and deploys via:
///   `CREATE2(value=0, offset, size, salt)`
///
/// The resulting address is:
///   `keccak256(0xff ++ factory ++ salt ++ keccak256(initcode))[12..]`
pub(crate) fn compute_address(salt: B256, initcode: &[u8]) -> Address {
    let init_code_hash = keccak256(initcode);
    DETERMINISTIC_DEPLOYER.create2(salt, init_code_hash)
}

/// Build the calldata to send to the deterministic deployer factory.
/// Format: `salt (32 bytes) ++ initcode`
pub(crate) fn build_factory_calldata(salt: B256, initcode: &[u8]) -> Bytes {
    let mut data = Vec::with_capacity(32 + initcode.len());
    data.extend_from_slice(salt.as_slice());
    data.extend_from_slice(initcode);
    Bytes::from(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::hex;

    #[test]
    fn known_create2_address() {
        let salt = B256::ZERO;
        // Minimal initcode: PUSH1 0x00 PUSH1 0x00 RETURN (returns empty code)
        let initcode = hex!("60006000f3");
        let addr = compute_address(salt, &initcode);

        let init_hash = keccak256(initcode);
        let expected = DETERMINISTIC_DEPLOYER.create2(salt, init_hash);
        assert_eq!(addr, expected);
    }

    #[test]
    fn different_salts_different_addresses() {
        let initcode = hex!("60006000f3");
        let addr1 = compute_address(B256::ZERO, &initcode);
        let addr2 = compute_address(B256::with_last_byte(1), &initcode);
        assert_ne!(addr1, addr2);
    }

    #[test]
    fn different_initcode_different_addresses() {
        let salt = B256::ZERO;
        let addr1 = compute_address(salt, &hex!("60006000f3"));
        let addr2 = compute_address(salt, &hex!("60016000f3"));
        assert_ne!(addr1, addr2);
    }

    #[test]
    fn factory_calldata_format() {
        let salt = B256::with_last_byte(0x42);
        let initcode = hex!("aabbcc");
        let calldata = build_factory_calldata(salt, &initcode);

        assert_eq!(calldata.len(), 32 + 3);
        assert_eq!(&calldata[..32], salt.as_slice());
        assert_eq!(&calldata[32..], &hex!("aabbcc"));
    }
}
