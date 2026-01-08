// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {AdminProxy} from "../src/AdminProxy.sol";

/// @title GenerateAdminProxyAlloc
/// @notice Generates genesis alloc JSON for deploying AdminProxy at a deterministic address
/// @dev Run with: forge script script/GenerateAdminProxyAlloc.s.sol -vvv
///
/// This script outputs the bytecode and storage layout needed to deploy AdminProxy
/// in the genesis block. The contract is deployed with owner = address(0), allowing
/// the first caller to claim ownership post-genesis.
///
/// Usage:
/// 1. Run this script to get the bytecode
/// 2. Add to genesis.json alloc section at desired address (e.g., 0x...AD00)
/// 3. Set that address as mintAdmin in chainspec config
contract GenerateAdminProxyAlloc is Script {
    // Suggested deterministic address for AdminProxy
    // Using a memorable address in the precompile-adjacent range
    address constant SUGGESTED_ADDRESS = 0x000000000000000000000000000000000000Ad00;

    function run() external {
        // Deploy to get runtime bytecode
        AdminProxy proxy = new AdminProxy();

        // Get runtime bytecode (not creation code)
        bytes memory runtimeCode = address(proxy).code;

        console.log("========== AdminProxy Genesis Alloc ==========");
        console.log("");
        console.log("Suggested address:", SUGGESTED_ADDRESS);
        console.log("");
        console.log("Add this to your genesis.json 'alloc' section:");
        console.log("");
        console.log("{");
        console.log('  "alloc": {');
        console.log('    "000000000000000000000000000000000000Ad00": {');
        console.log('      "balance": "0x0",');
        console.log('      "code": "0x%s",', vm.toString(runtimeCode));
        console.log('      "storage": {}');
        console.log("    }");
        console.log("  }");
        console.log("}");
        console.log("");
        console.log("Then update chainspec config:");
        console.log("");
        console.log('{');
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
        console.log("1. Call claimOwnership() from desired EOA");
        console.log("2. Deploy multisig (e.g., Safe)");
        console.log("3. Call transferOwnership(multisigAddress)");
        console.log("4. From multisig, call acceptOwnership()");
        console.log("");

        // Also output raw values for programmatic use
        console.log("Raw bytecode length:", runtimeCode.length);
    }
}

/// @title GenerateAdminProxyAllocJSON
/// @notice Outputs just the JSON snippet for easy copy-paste
contract GenerateAdminProxyAllocJSON is Script {
    function run() external {
        AdminProxy proxy = new AdminProxy();
        bytes memory runtimeCode = address(proxy).code;

        // Output minimal JSON that can be merged into genesis
        string memory json = string(
            abi.encodePacked(
                '{"000000000000000000000000000000000000Ad00":{"balance":"0x0","code":"0x',
                vm.toString(runtimeCode),
                '","storage":{}}}'
            )
        );

        console.log(json);
    }
}
