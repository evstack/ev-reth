// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test, console} from "forge-std/Test.sol";
import {FeeVault} from "../src/FeeVault.sol";

contract MockHypNativeMinter {
    event TransferRemoteCalled(uint32 destination, bytes32 recipient, uint256 amount);

    function transferRemote(uint32 _destination, bytes32 _recipient, uint256 _amount)
        external
        payable
        returns (bytes32 messageId)
    {
        require(msg.value == _amount, "MockHypNativeMinter: value mismatch");
        emit TransferRemoteCalled(_destination, _recipient, _amount);
        return bytes32(uint256(1)); // Return a dummy messageId
    }
}

contract FeeVaultTest is Test {
    FeeVault public feeVault;
    MockHypNativeMinter public mockMinter;
    address public owner;
    address public user;
    address public otherRecipient;

    uint32 public destination = 1234;
    bytes32 public recipient = bytes32(uint256(0xdeadbeef));
    uint256 public minAmount = 1 ether;
    uint256 public fee = 0.1 ether;

    function setUp() public {
        owner = address(this);
        user = address(0x1);
        otherRecipient = address(0x99);
        mockMinter = new MockHypNativeMinter();
        feeVault = new FeeVault(address(mockMinter), owner);

        // Configure contract
        feeVault.setRecipient(destination, recipient);
        feeVault.setMinimumAmount(minAmount);
        feeVault.setCallFee(fee);
        feeVault.setOtherRecipient(otherRecipient);
        // Default bridge share is 10000 (100%)
    }

    function test_Receive() public {
        uint256 amount = 1 ether;
        (bool success,) = address(feeVault).call{value: amount}("");
        assertTrue(success, "Transfer failed");
        assertEq(address(feeVault).balance, amount, "Balance mismatch");
    }

    function test_SendToCelestia_100PercentBridge() public {
        // Fund with minAmount
        (bool success,) = address(feeVault).call{value: minAmount}("");
        require(success);

        uint256 totalAmount = minAmount + fee;

        vm.expectEmit(true, true, true, true, address(mockMinter));
        emit MockHypNativeMinter.TransferRemoteCalled(destination, recipient, totalAmount);

        // Expect the event from FeeVault
        vm.expectEmit(true, true, true, true, address(feeVault));
        emit FeeVault.SentToCelestia(totalAmount, recipient, bytes32(uint256(1)));

        vm.prank(user);
        vm.deal(user, fee);
        feeVault.sendToCelestia{value: fee}();

        assertEq(address(feeVault).balance, 0, "Collector should be empty");
    }

    function test_SendToCelestia_Split5050() public {
        // Set split to 50%
        feeVault.setBridgeShare(5000);

        // Fund with 2 ether.
        // Fee is 0.1 ether.
        // Total new funds = 2.1 ether.
        // Bridge = 1.05 ether. Other = 1.05 ether.
        // Min amount is 1 ether, so 1.05 >= 1.0 is OK.
        uint256 fundAmount = 2 ether;
        (bool success,) = address(feeVault).call{value: fundAmount}("");
        require(success);

        uint256 totalNew = fundAmount + fee;
        uint256 expectedBridge = totalNew / 2;
        uint256 expectedOther = totalNew - expectedBridge;

        vm.expectEmit(true, true, true, true, address(mockMinter));
        emit MockHypNativeMinter.TransferRemoteCalled(destination, recipient, expectedBridge);

        vm.prank(user);
        vm.deal(user, fee);
        feeVault.sendToCelestia{value: fee}();

        assertEq(address(feeVault).balance, 0, "Collector should be empty");
        assertEq(otherRecipient.balance, expectedOther, "Other recipient should receive funds");
    }

    function test_SendToCelestia_InsufficientFee() public {
        vm.prank(user);
        vm.deal(user, fee);
        // Send less than fee
        vm.expectRevert("FeeVault: insufficient fee");
        feeVault.sendToCelestia{value: fee - 1}();
    }

    function test_SendToCelestia_BelowMinAmount_AfterSplit() public {
        feeVault.setBridgeShare(1000); // 10% bridge

        // Fund with 2 ether. Total 2.1.
        // Bridge = 0.21. Other = 1.89.
        // Min amount is 1.0. 0.21 < 1.0. Should revert.
        (bool success,) = address(feeVault).call{value: 2 ether}("");
        require(success);

        vm.prank(user);
        vm.deal(user, fee);
        vm.expectRevert("FeeVault: minimum amount not met");
        feeVault.sendToCelestia{value: fee}();
    }

    function test_AdminFunctions() public {
        // Test setRecipient
        feeVault.setRecipient(5678, bytes32(uint256(0xbeef)));
        assertEq(feeVault.destinationDomain(), 5678);
        assertEq(feeVault.recipientAddress(), bytes32(uint256(0xbeef)));

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
        feeVault.setRecipient(1, bytes32(0));

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
    }
}
