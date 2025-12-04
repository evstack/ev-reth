// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {FeeVault} from "../src/FeeVault.sol";

contract DeployFeeVault is Script {
    function run() external {
        // ========== CONFIGURATION ==========
        address owner = vm.envAddress("OWNER");
        bytes32 salt = vm.envOr("SALT", bytes32(0));

        // Optional: Post-deployment configuration
        uint32 destinationDomain = uint32(vm.envOr("DESTINATION_DOMAIN", uint256(0)));
        bytes32 recipientAddress = vm.envOr("RECIPIENT_ADDRESS", bytes32(0));
        uint256 minimumAmount = vm.envOr("MINIMUM_AMOUNT", uint256(0));
        uint256 callFee = vm.envOr("CALL_FEE", uint256(0));
        uint256 bridgeShareBps = vm.envOr("BRIDGE_SHARE_BPS", uint256(10000));
        address otherRecipient = vm.envOr("OTHER_RECIPIENT", address(0));
        // ===================================

        // Compute address before deployment
        address predicted = computeAddress(salt, owner);
        console.log("Predicted FeeVault address:", predicted);

        vm.startBroadcast();

        // Deploy FeeVault with CREATE2
        FeeVault feeVault = new FeeVault{salt: salt}(owner);
        console.log("FeeVault deployed at:", address(feeVault));
        require(address(feeVault) == predicted, "Address mismatch");

        // Configure if values provided
        if (destinationDomain != 0 && recipientAddress != bytes32(0)) {
            feeVault.setRecipient(destinationDomain, recipientAddress);
            console.log("Recipient set - domain:", destinationDomain);
        }

        if (minimumAmount > 0) {
            feeVault.setMinimumAmount(minimumAmount);
            console.log("Minimum amount set:", minimumAmount);
        }

        if (callFee > 0) {
            feeVault.setCallFee(callFee);
            console.log("Call fee set:", callFee);
        }

        if (bridgeShareBps != 10000) {
            feeVault.setBridgeShare(bridgeShareBps);
            console.log("Bridge share set:", bridgeShareBps, "bps");
        }

        if (otherRecipient != address(0)) {
            feeVault.setOtherRecipient(otherRecipient);
            console.log("Other recipient set:", otherRecipient);
        }

        vm.stopBroadcast();

        console.log("");
        console.log("NOTE: Call setHypNativeMinter() after deploying HypNativeMinter");
    }

    /// @notice Compute the CREATE2 address for FeeVault deployment
    function computeAddress(bytes32 salt, address owner) public view returns (address) {
        bytes32 bytecodeHash = keccak256(abi.encodePacked(type(FeeVault).creationCode, abi.encode(owner)));
        return address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), address(this), salt, bytecodeHash)))));
    }
}

/// @notice Standalone script to compute FeeVault address without deploying
contract ComputeFeeVaultAddress is Script {
    function run() external view {
        address owner = vm.envAddress("OWNER");
        bytes32 salt = vm.envOr("SALT", bytes32(0));
        address deployer = vm.envAddress("DEPLOYER");

        bytes32 bytecodeHash = keccak256(abi.encodePacked(type(FeeVault).creationCode, abi.encode(owner)));

        address predicted =
            address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), deployer, salt, bytecodeHash)))));

        console.log("========== FeeVault Address Computation ==========");
        console.log("Owner:", owner);
        console.log("Salt:", vm.toString(salt));
        console.log("Deployer:", deployer);
        console.log("Predicted address:", predicted);
        console.log("==================================================");
    }
}
