//! Deterministic deployer (Nick's factory) bytecode for genesis injection.

use crate::{config::DeterministicDeployerConfig, contracts::GenesisContract};
use alloy_primitives::{hex, Bytes};
use std::collections::BTreeMap;

/// Runtime bytecode of the deterministic deployer factory from Ethereum mainnet.
/// See: <https://github.com/Arachnid/deterministic-deployment-proxy>
pub(crate) const DETERMINISTIC_DEPLOYER_BYTECODE: &[u8] = &hex!(
    "7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe03601600081602082378035828234f58015156039578182fd5b80825250506014600cf3"
);

/// Build a genesis alloc entry for the deterministic deployer.
pub(crate) fn build(config: &DeterministicDeployerConfig) -> GenesisContract {
    let address = config.address.expect("address required for genesis");

    GenesisContract {
        address,
        code: Bytes::from_static(DETERMINISTIC_DEPLOYER_BYTECODE),
        storage: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn build_produces_correct_bytecode() {
        let config = DeterministicDeployerConfig {
            address: Some(address!("4e59b44847b379578588920ca78fbf26c0b4956c")),
        };
        let contract = build(&config);

        assert_eq!(contract.address, address!("4e59b44847b379578588920ca78fbf26c0b4956c"));
        assert_eq!(contract.code.as_ref(), DETERMINISTIC_DEPLOYER_BYTECODE);
        assert!(contract.storage.is_empty());
    }
}
