// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {AdminProxy} from "../src/AdminProxy.sol";

/// @title GenerateAdminProxyAlloc
/// @notice Generates genesis alloc JSON for deploying AdminProxy at a deterministic address
/// @dev Run with: OWNER=0xYourAddress forge script script/GenerateAdminProxyAlloc.s.sol -vvv
///
/// This script outputs the bytecode and storage layout needed to deploy AdminProxy
/// in the genesis block. The owner is set directly in storage slot 0.
///
/// Usage:
/// 1. Set OWNER env var to your initial admin EOA address
/// 2. Run this script to get the bytecode and storage
/// 3. Add to genesis.json alloc section at desired address (e.g., 0x...Ad00)
/// 4. Set that address as mintAdmin in chainspec config
contract GenerateAdminProxyAlloc is Script {
    // Suggested deterministic address for AdminProxy
    // Using a memorable address in the precompile-adjacent range
    address constant SUGGESTED_ADDRESS = 0x000000000000000000000000000000000000Ad00;

    function run() external {
        // Get owner from environment, default to zero if not set
        address owner = vm.envOr("OWNER", address(0));

        // Deploy to get runtime bytecode
        AdminProxy proxy = new AdminProxy();

        // Get runtime bytecode (not creation code)
        bytes memory runtimeCode = address(proxy).code;

        // Convert owner to storage slot value (left-padded to 32 bytes)
        bytes32 ownerSlotValue = bytes32(uint256(uint160(owner)));

        console.log("========== AdminProxy Genesis Alloc ==========");
        console.log("");
        console.log("Suggested address:", SUGGESTED_ADDRESS);
        console.log("Owner (from OWNER env):", owner);
        console.log("");

        if (owner == address(0)) {
            console.log("WARNING: OWNER not set! Set OWNER env var to your admin EOA.");
            console.log("Example: OWNER=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 forge script ...");
            console.log("");
        }

        console.log("Add this to your genesis.json 'alloc' section:");
        console.log("");
        console.log("{");
        console.log('  "alloc": {');
        console.log('    "000000000000000000000000000000000000Ad00": {');
        console.log('      "balance": "0x0",');
        console.log('      "code": "0x%s",', vm.toString(runtimeCode));
        console.log('      "storage": {');
        console.log('        "0x0": "0x%s"', vm.toString(ownerSlotValue));
        console.log("      }");
        console.log("    }");
        console.log("  }");
        console.log("}");
        console.log("");
        console.log("Then update chainspec config:");
        console.log("");
        console.log("{");
        console.log('  "config": {');
        console.log('    "evolve": {');
        console.log('      "mintAdmin": "0x000000000000000000000000000000000000Ad00",');
        console.log('      "mintPrecompileActivationHeight": 0');
        console.log("    }");
        console.log("  }");
        console.log("}");
        console.log("");
        console.log("==============================================");
        console.log("");
        console.log("Post-genesis steps:");
        console.log("1. Owner can immediately use the proxy (no claiming needed)");
        console.log("2. Deploy multisig (e.g., Safe)");
        console.log("3. Call transferOwnership(multisigAddress)");
        console.log("4. From multisig, call acceptOwnership()");
        console.log("");

        // Also output raw values for programmatic use
        console.log("Raw bytecode length:", runtimeCode.length);
        console.log("Owner storage slot (0x0):", vm.toString(ownerSlotValue));
    }
}

/// @title GenerateAdminProxyAllocJSON
/// @notice Outputs just the JSON snippet for easy copy-paste
/// @dev Run with: OWNER=0xYourAddress forge script script/GenerateAdminProxyAlloc.s.sol:GenerateAdminProxyAllocJSON -vvv
contract GenerateAdminProxyAllocJSON is Script {
    function run() external {
        address owner = vm.envOr("OWNER", address(0));

        AdminProxy proxy = new AdminProxy();
        bytes memory runtimeCode = address(proxy).code;
        bytes32 ownerSlotValue = bytes32(uint256(uint160(owner)));

        // Output minimal JSON that can be merged into genesis
        string memory json = string(
            abi.encodePacked(
                '{"000000000000000000000000000000000000Ad00":{"balance":"0x0","code":"0x',
                vm.toString(runtimeCode),
                '","storage":{"0x0":"0x',
                vm.toString(ownerSlotValue),
                '"}}}'
            )
        );

        console.log(json);
    }
}
