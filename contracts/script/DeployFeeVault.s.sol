// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {FeeVault} from "../src/FeeVault.sol";

contract DeployFeeVault is Script {
    function run() external {
        // ========== CONFIGURATION ==========
        address owner = vm.envAddress("OWNER");
        bytes32 salt = vm.envOr("SALT", bytes32(0));

        uint32 destinationDomain = uint32(vm.envOr("DESTINATION_DOMAIN", uint256(0)));
        bytes32 recipientAddress = vm.envOr("RECIPIENT_ADDRESS", bytes32(0));
        uint256 minimumAmount = vm.envOr("MINIMUM_AMOUNT", uint256(0));
        uint256 callFee = vm.envOr("CALL_FEE", uint256(0));
        uint256 bridgeShareBps = vm.envOr("BRIDGE_SHARE_BPS", uint256(0)); // 0 defaults to 10000 in constructor
        address otherRecipient = vm.envOr("OTHER_RECIPIENT", address(0));
        // ===================================

        vm.startBroadcast();

        // Deploy FeeVault with CREATE2
        FeeVault feeVault = new FeeVault{salt: salt}(
            owner, destinationDomain, recipientAddress, minimumAmount, callFee, bridgeShareBps, otherRecipient
        );

        vm.stopBroadcast();

        console.log("FeeVault deployed at:", address(feeVault));
        console.log("Owner:", owner);
        console.log("Destination domain:", destinationDomain);
        console.log("Minimum amount:", minimumAmount);
        console.log("Call fee:", callFee);
        console.log("Bridge share bps:", feeVault.bridgeShareBps());
        console.log("");
        console.log("NOTE: Call setHypNativeMinter() after deploying HypNativeMinter");
    }
}

/// @notice Compute FeeVault CREATE2 address off-chain
/// @dev Use this to predict the address before deploying
///      Requires env vars: DEPLOYER (EOA), OWNER, SALT (optional), and all constructor args
contract ComputeFeeVaultAddress is Script {
    function run() external view {
        address deployer = vm.envAddress("DEPLOYER");
        bytes32 salt = vm.envOr("SALT", bytes32(0));

        address owner = vm.envAddress("OWNER");
        uint32 destinationDomain = uint32(vm.envOr("DESTINATION_DOMAIN", uint256(0)));
        bytes32 recipientAddress = vm.envOr("RECIPIENT_ADDRESS", bytes32(0));
        uint256 minimumAmount = vm.envOr("MINIMUM_AMOUNT", uint256(0));
        uint256 callFee = vm.envOr("CALL_FEE", uint256(0));
        uint256 bridgeShareBps = vm.envOr("BRIDGE_SHARE_BPS", uint256(0));
        address otherRecipient = vm.envOr("OTHER_RECIPIENT", address(0));

        bytes32 initCodeHash = keccak256(
            abi.encodePacked(
                type(FeeVault).creationCode,
                abi.encode(
                    owner, destinationDomain, recipientAddress, minimumAmount, callFee, bridgeShareBps, otherRecipient
                )
            )
        );

        address predicted =
            address(uint160(uint256(keccak256(abi.encodePacked(bytes1(0xff), deployer, salt, initCodeHash)))));

        console.log("========== FeeVault Address Computation ==========");
        console.log("Deployer (EOA):", deployer);
        console.log("Owner:", owner);
        console.log("Salt:", vm.toString(salt));
        console.log("Predicted address:", predicted);
        console.log("==================================================");
    }
}
