// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test, console} from "forge-std/Test.sol";
import {FeeVault} from "../src/FeeVault.sol";

contract FeeVaultTest is Test {
    FeeVault public feeVault;
    address public owner;
    address public user;
    address public bridgeRecipient;
    address public otherRecipient;

    uint256 public minAmount = 1 ether;
    uint256 public fee = 0.1 ether;

    function setUp() public {
        owner = address(this);
        user = address(0x1);
        bridgeRecipient = address(0x42);
        otherRecipient = address(0x99);

        feeVault = new FeeVault(
            owner,
            minAmount,
            fee,
            10000, // 100% bridge share
            otherRecipient
        );

        feeVault.setBridgeRecipient(bridgeRecipient);
    }

    function test_GetConfig() public view {
        (
            address cfgOwner,
            address cfgBridgeRecipient,
            address cfgOtherRecipient,
            uint256 cfgMinAmount,
            uint256 cfgCallFee,
            uint256 cfgBridgeShare
        ) = feeVault.getConfig();

        assertEq(cfgOwner, owner);
        assertEq(cfgBridgeRecipient, bridgeRecipient);
        assertEq(cfgOtherRecipient, otherRecipient);
        assertEq(cfgMinAmount, minAmount);
        assertEq(cfgCallFee, fee);
        assertEq(cfgBridgeShare, 10000);
    }

    function test_Receive() public {
        uint256 amount = 1 ether;
        (bool success,) = address(feeVault).call{value: amount}("");
        assertTrue(success, "Transfer failed");
        assertEq(address(feeVault).balance, amount, "Balance mismatch");
    }

    function test_Distribute_100PercentBridge() public {
        // Fund with minAmount
        (bool success,) = address(feeVault).call{value: minAmount}("");
        require(success);

        uint256 totalAmount = minAmount + fee;

        vm.expectEmit(true, true, true, true, address(feeVault));
        emit FeeVault.FundsDistributed(totalAmount, totalAmount, 0);

        vm.prank(user);
        vm.deal(user, fee);
        feeVault.distribute{value: fee}();

        assertEq(address(feeVault).balance, 0, "Vault should be empty");
        assertEq(bridgeRecipient.balance, totalAmount, "Bridge recipient should receive funds");
    }

    function test_Distribute_Split5050() public {
        // Set split to 50%
        feeVault.setBridgeShare(5000);

        uint256 fundAmount = 2 ether;
        (bool success,) = address(feeVault).call{value: fundAmount}("");
        require(success);

        uint256 totalNew = fundAmount + fee;
        uint256 expectedBridge = totalNew / 2;
        uint256 expectedOther = totalNew - expectedBridge;

        vm.prank(user);
        vm.deal(user, fee);
        feeVault.distribute{value: fee}();

        assertEq(address(feeVault).balance, 0, "Vault should be empty");
        assertEq(bridgeRecipient.balance, expectedBridge, "Bridge recipient should receive funds");
        assertEq(otherRecipient.balance, expectedOther, "Other recipient should receive funds");
    }

    function test_Distribute_InsufficientFee() public {
        vm.prank(user);
        vm.deal(user, fee);
        vm.expectRevert("FeeVault: insufficient fee");
        feeVault.distribute{value: fee - 1}();
    }

    function test_Distribute_BelowMinAmount_AfterSplit() public {
        feeVault.setBridgeShare(1000); // 10% bridge

        (bool success,) = address(feeVault).call{value: 2 ether}("");
        require(success);

        vm.prank(user);
        vm.deal(user, fee);
        vm.expectRevert("FeeVault: minimum amount not met");
        feeVault.distribute{value: fee}();
    }

    function test_Distribute_BridgeRecipientNotSet() public {
        FeeVault freshVault = new FeeVault(owner, minAmount, fee, 10000, otherRecipient);

        (bool success,) = address(freshVault).call{value: minAmount}("");
        require(success);

        vm.prank(user);
        vm.deal(user, fee);
        vm.expectRevert("FeeVault: bridge recipient not set");
        freshVault.distribute{value: fee}();
    }

    function test_AdminFunctions() public {
        // Test setMinimumAmount
        feeVault.setMinimumAmount(5 ether);
        assertEq(feeVault.minimumAmount(), 5 ether);

        // Test setCallFee
        feeVault.setCallFee(1 ether);
        assertEq(feeVault.callFee(), 1 ether);

        // Test transferOwnership
        address newOwner = address(0x2);
        feeVault.transferOwnership(newOwner);
        assertEq(feeVault.owner(), newOwner);

        // Old owner cannot call anymore
        vm.expectRevert("FeeVault: caller is not the owner");
        feeVault.setCallFee(2 ether);

        // Test setBridgeShare
        vm.prank(newOwner);
        feeVault.setBridgeShare(5000);
        assertEq(feeVault.bridgeShareBps(), 5000);

        vm.prank(newOwner);
        vm.expectRevert("FeeVault: invalid bps");
        feeVault.setBridgeShare(10001);

        // Test setOtherRecipient
        vm.prank(newOwner);
        address newOther = address(0x88);
        feeVault.setOtherRecipient(newOther);
        assertEq(feeVault.otherRecipient(), newOther);
    }

    function test_AdminAccessControl() public {
        vm.prank(user);
        vm.expectRevert("FeeVault: caller is not the owner");
        feeVault.setMinimumAmount(1);

        vm.prank(user);
        vm.expectRevert("FeeVault: caller is not the owner");
        feeVault.setCallFee(1);

        vm.prank(user);
        vm.expectRevert("FeeVault: caller is not the owner");
        feeVault.transferOwnership(user);

        vm.prank(user);
        vm.expectRevert("FeeVault: caller is not the owner");
        feeVault.setBridgeShare(5000);

        vm.prank(user);
        vm.expectRevert("FeeVault: caller is not the owner");
        feeVault.setOtherRecipient(user);

        vm.prank(user);
        vm.expectRevert("FeeVault: caller is not the owner");
        feeVault.setBridgeRecipient(address(0x123));
    }

    function test_SetBridgeRecipient() public {
        address newRecipient = address(0x55);
        feeVault.setBridgeRecipient(newRecipient);
        assertEq(feeVault.bridgeRecipient(), newRecipient);
    }

    function test_SetBridgeRecipient_ZeroAddress() public {
        vm.expectRevert("FeeVault: zero address");
        feeVault.setBridgeRecipient(address(0));
    }
}
