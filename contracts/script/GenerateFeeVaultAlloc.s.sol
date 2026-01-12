// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {FeeVault} from "../src/FeeVault.sol";

abstract contract FeeVaultAllocBase is Script {
    struct Config {
        address feeVaultAddress;
        address owner;
        uint32 destinationDomain;
        bytes32 recipientAddress;
        uint256 minimumAmount;
        uint256 callFee;
        uint256 bridgeShareBpsRaw;
        uint256 bridgeShareBps;
        address otherRecipient;
        address hypNativeMinter;
        bytes32 salt;
        address deployer;
    }

    function loadConfig() internal view returns (Config memory cfg) {
        cfg.owner = vm.envAddress("OWNER");
        cfg.destinationDomain = uint32(vm.envOr("DESTINATION_DOMAIN", uint256(0)));
        cfg.recipientAddress = vm.envOr("RECIPIENT_ADDRESS", bytes32(0));
        cfg.minimumAmount = vm.envOr("MINIMUM_AMOUNT", uint256(0));
        cfg.callFee = vm.envOr("CALL_FEE", uint256(0));
        cfg.bridgeShareBpsRaw = vm.envOr("BRIDGE_SHARE_BPS", uint256(0));
        cfg.otherRecipient = vm.envOr("OTHER_RECIPIENT", address(0));
        cfg.hypNativeMinter = vm.envOr("HYP_NATIVE_MINTER", address(0));
        cfg.feeVaultAddress = vm.envOr("FEE_VAULT_ADDRESS", address(0));
        cfg.deployer = vm.envOr("DEPLOYER", address(0));
        cfg.salt = vm.envOr("SALT", bytes32(0));

        require(cfg.owner != address(0), "OWNER required");
        require(cfg.bridgeShareBpsRaw <= 10000, "BRIDGE_SHARE_BPS > 10000");

        cfg.bridgeShareBps = cfg.bridgeShareBpsRaw == 0 ? 10000 : cfg.bridgeShareBpsRaw;

        if (cfg.feeVaultAddress == address(0) && cfg.deployer != address(0)) {
            bytes32 initCodeHash = keccak256(
                abi.encodePacked(
                    type(FeeVault).creationCode,
                    abi.encode(
                        cfg.owner,
                        cfg.destinationDomain,
                        cfg.recipientAddress,
                        cfg.minimumAmount,
                        cfg.callFee,
                        cfg.bridgeShareBpsRaw,
                        cfg.otherRecipient
                    )
                )
            );
            cfg.feeVaultAddress = address(
                uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), cfg.deployer, cfg.salt, initCodeHash))))
            );
        }

        require(cfg.feeVaultAddress != address(0), "FEE_VAULT_ADDRESS or DEPLOYER required");
    }

    function computeSlots(Config memory cfg)
        internal
        pure
        returns (
            bytes32 slot0,
            bytes32 slot1,
            bytes32 slot2,
            bytes32 slot3,
            bytes32 slot4,
            bytes32 slot5,
            bytes32 slot6
        )
    {
        slot0 = bytes32(uint256(uint160(cfg.hypNativeMinter)));
        slot1 = bytes32((uint256(cfg.destinationDomain) << 160) | uint256(uint160(cfg.owner)));
        slot2 = cfg.recipientAddress;
        slot3 = bytes32(cfg.minimumAmount);
        slot4 = bytes32(cfg.callFee);
        slot5 = bytes32(uint256(uint160(cfg.otherRecipient)));
        slot6 = bytes32(cfg.bridgeShareBps);
    }

    function addressKey(address addr) internal pure returns (string memory) {
        bytes memory full = bytes(vm.toString(addr));
        bytes memory key = new bytes(40);
        // Fixed-length copy for address key without 0x prefix.
        for (uint256 i = 0; i < 40; i++) {
            key[i] = full[i + 2];
        }
        return string(key);
    }
}

/// @title GenerateFeeVaultAlloc
/// @notice Generates genesis alloc JSON for deploying FeeVault at a deterministic address
/// @dev Run with: OWNER=0x... forge script script/GenerateFeeVaultAlloc.s.sol -vvv
contract GenerateFeeVaultAlloc is FeeVaultAllocBase {
    function run() external view {
        Config memory cfg = loadConfig();
        bytes memory runtimeCode = type(FeeVault).runtimeCode;

        (bytes32 slot0, bytes32 slot1, bytes32 slot2, bytes32 slot3, bytes32 slot4, bytes32 slot5, bytes32 slot6) =
            computeSlots(cfg);

        console.log("========== FeeVault Genesis Alloc ==========");
        console.log("FeeVault address:", cfg.feeVaultAddress);
        console.log("Owner:", cfg.owner);
        console.log("Destination domain:", cfg.destinationDomain);
        console.log("Bridge share bps (raw):", cfg.bridgeShareBpsRaw);
        console.log("Bridge share bps (effective):", cfg.bridgeShareBps);
        console.log("");

        if (cfg.bridgeShareBpsRaw == 0) {
            console.log("NOTE: BRIDGE_SHARE_BPS=0 defaults to 10000 (constructor behavior).");
        }
        if (cfg.bridgeShareBps < 10000 && cfg.otherRecipient == address(0)) {
            console.log("WARNING: OTHER_RECIPIENT is zero but bridge share < 10000.");
        }
        if (cfg.hypNativeMinter == address(0)) {
            console.log("NOTE: HYP_NATIVE_MINTER is zero; set it before calling sendToCelestia().");
        }
        console.log("");

        console.log("Add this to your genesis.json 'alloc' section:");
        console.log("");
        console.log("{");
        console.log('  "alloc": {');
        console.log('    "%s": {', addressKey(cfg.feeVaultAddress));
        console.log('      "balance": "0x0",');
        console.log('      "code": "0x%s",', vm.toString(runtimeCode));
        console.log('      "storage": {');
        console.log('        "0x0": "0x%s",', vm.toString(slot0));
        console.log('        "0x1": "0x%s",', vm.toString(slot1));
        console.log('        "0x2": "0x%s",', vm.toString(slot2));
        console.log('        "0x3": "0x%s",', vm.toString(slot3));
        console.log('        "0x4": "0x%s",', vm.toString(slot4));
        console.log('        "0x5": "0x%s",', vm.toString(slot5));
        console.log('        "0x6": "0x%s"', vm.toString(slot6));
        console.log("      }");
        console.log("    }");
        console.log("  }");
        console.log("}");
        console.log("");
        console.log("Raw bytecode length:", runtimeCode.length);
        console.log("=============================================");
    }
}

/// @title GenerateFeeVaultAllocJSON
/// @notice Outputs just the JSON snippet for easy copy-paste
/// @dev Run with: OWNER=0x... forge script script/GenerateFeeVaultAlloc.s.sol:GenerateFeeVaultAllocJSON -vvv
contract GenerateFeeVaultAllocJSON is FeeVaultAllocBase {
    function run() external view {
        Config memory cfg = loadConfig();
        bytes memory runtimeCode = type(FeeVault).runtimeCode;

        (bytes32 slot0, bytes32 slot1, bytes32 slot2, bytes32 slot3, bytes32 slot4, bytes32 slot5, bytes32 slot6) =
            computeSlots(cfg);

        string memory json = string(
            abi.encodePacked(
                '{"',
                addressKey(cfg.feeVaultAddress),
                '":{"balance":"0x0","code":"0x',
                vm.toString(runtimeCode),
                '","storage":{"0x0":"0x',
                vm.toString(slot0),
                '","0x1":"0x',
                vm.toString(slot1),
                '","0x2":"0x',
                vm.toString(slot2),
                '","0x3":"0x',
                vm.toString(slot3),
                '","0x4":"0x',
                vm.toString(slot4),
                '","0x5":"0x',
                vm.toString(slot5),
                '","0x6":"0x',
                vm.toString(slot6),
                '"}}}'
            )
        );

        console.log(json);
    }
}
