// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {FeeVault} from "../src/FeeVault.sol";

contract DeployFeeVault is Script {
    function run() external {
        // ========== CONFIGURATION ==========
        address owner = vm.envAddress("OWNER");

        // Optional: Post-deployment configuration
        uint32 destinationDomain = uint32(vm.envOr("DESTINATION_DOMAIN", uint256(0)));
        bytes32 recipientAddress = vm.envOr("RECIPIENT_ADDRESS", bytes32(0));
        uint256 minimumAmount = vm.envOr("MINIMUM_AMOUNT", uint256(0));
        uint256 callFee = vm.envOr("CALL_FEE", uint256(0));
        uint256 bridgeShareBps = vm.envOr("BRIDGE_SHARE_BPS", uint256(10000));
        address otherRecipient = vm.envOr("OTHER_RECIPIENT", address(0));
        // ===================================

        vm.startBroadcast();

        // Deploy FeeVault
        FeeVault feeVault = new FeeVault(owner);
        console.log("FeeVault deployed at:", address(feeVault));

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
}
