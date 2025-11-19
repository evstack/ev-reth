// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test, console} from "forge-std/Test.sol";
import {FeeVault} from "../src/FeeVault.sol";

contract MockHypNativeMinter {
    event TransferRemoteCalled(uint32 destination, bytes32 recipient, uint256 amount);

    function transferRemote(
        uint32 _destination,
        bytes32 _recipient,
        uint256 _amount
    ) external payable returns (bytes32 messageId) {
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

    uint32 public destination = 1234;
    bytes32 public recipient = bytes32(uint256(0xdeadbeef));
    uint256 public minAmount = 1 ether;
    uint256 public fee = 0.1 ether;

    function setUp() public {
        owner = address(this);
        user = address(0x1);
        mockMinter = new MockHypNativeMinter();
        feeVault = new FeeVault(address(mockMinter), owner);

        // Configure contract
        feeVault.setRecipient(destination, recipient);
        feeVault.setMinimumAmount(minAmount);
        feeVault.setCallFee(fee);
        // Default burn share is 10000 (100%)
    }

    function test_Receive() public {
        uint256 amount = 1 ether;
        (bool success, ) = address(feeVault).call{value: amount}("");
        assertTrue(success, "Transfer failed");
        assertEq(address(feeVault).balance, amount, "Balance mismatch");
    }

    function test_SendToCelestia_100PercentBurn() public {
        // Fund with minAmount
        (bool success, ) = address(feeVault).call{value: minAmount}("");
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
        assertEq(feeVault.otherBucketBalance(), 0, "Other bucket should be empty");
    }

    function test_SendToCelestia_Split5050() public {
        // Set split to 50%
        feeVault.setBurnShare(5000);

        // Fund with 2 ether. 
        // Fee is 0.1 ether.
        // Total new funds = 2.1 ether.
        // Burn = 1.05 ether. Other = 1.05 ether.
        // Min amount is 1 ether, so 1.05 >= 1.0 is OK.
        uint256 fundAmount = 2 ether;
        (bool success, ) = address(feeVault).call{value: fundAmount}("");
        require(success);

        uint256 totalNew = fundAmount + fee;
        uint256 expectedBurn = totalNew / 2;
        uint256 expectedOther = totalNew - expectedBurn;

        vm.expectEmit(true, true, true, true, address(mockMinter));
        emit MockHypNativeMinter.TransferRemoteCalled(destination, recipient, expectedBurn);

        vm.prank(user);
        vm.deal(user, fee);
        feeVault.sendToCelestia{value: fee}();

        assertEq(address(feeVault).balance, expectedOther, "Collector should hold other funds");
        assertEq(feeVault.otherBucketBalance(), expectedOther, "Other bucket accounting incorrect");
    }

    function test_SendToCelestia_AccumulateOther() public {
        feeVault.setBurnShare(5000); // 50%

        // First call: 2 ether + 0.1 fee = 2.1 total. 1.05 burn, 1.05 other.
        (bool success, ) = address(feeVault).call{value: 2 ether}("");
        require(success);
        
        vm.prank(user);
        vm.deal(user, fee);
        feeVault.sendToCelestia{value: fee}();

        uint256 firstOther = 1.05 ether;
        assertEq(feeVault.otherBucketBalance(), firstOther);

        // Second call: 2 ether + 0.1 fee = 2.1 total NEW funds.
        // Previous balance: 1.05. New balance before split: 1.05 + 2.1 = 3.15.
        // Logic: newFunds = balance (3.15) - otherBucket (1.05) = 2.1. Correct.
        (success, ) = address(feeVault).call{value: 2 ether}("");
        require(success);

        vm.prank(user);
        vm.deal(user, fee);
        feeVault.sendToCelestia{value: fee}();

        uint256 secondOther = 1.05 ether;
        assertEq(feeVault.otherBucketBalance(), firstOther + secondOther);
        assertEq(address(feeVault).balance, firstOther + secondOther);
    }

    function test_WithdrawOther() public {
        feeVault.setBurnShare(5000);
        (bool success, ) = address(feeVault).call{value: 2 ether}("");
        require(success);

        vm.prank(user);
        vm.deal(user, fee);
        feeVault.sendToCelestia{value: fee}();

        uint256 otherAmount = 1.05 ether;
        assertEq(feeVault.otherBucketBalance(), otherAmount);

        // Withdraw half
        uint256 withdrawAmount = 0.5 ether;
        address payable recipientAddr = payable(address(0x99));
        
        feeVault.withdrawOther(recipientAddr, withdrawAmount);

        assertEq(feeVault.otherBucketBalance(), otherAmount - withdrawAmount);
        assertEq(recipientAddr.balance, withdrawAmount);
        assertEq(address(feeVault).balance, otherAmount - withdrawAmount);
    }

    function test_WithdrawOther_Insufficient() public {
        feeVault.setBurnShare(5000);
        (bool success, ) = address(feeVault).call{value: 2 ether}("");
        require(success);
        
        vm.prank(user);
        vm.deal(user, fee);
        feeVault.sendToCelestia{value: fee}();

        uint256 otherAmount = 1.05 ether;
        
        vm.expectRevert("FeeVault: insufficient other balance");
        feeVault.withdrawOther(payable(owner), otherAmount + 1);
    }

    function test_SendToCelestia_InsufficientFee() public {
        vm.prank(user);
        vm.deal(user, fee);
        // Send less than fee
        vm.expectRevert("FeeVault: insufficient fee");
        feeVault.sendToCelestia{value: fee - 1}();
    }

    function test_SendToCelestia_BelowMinAmount_AfterSplit() public {
        feeVault.setBurnShare(1000); // 10% burn
        
        // Fund with 2 ether. Total 2.1.
        // Burn = 0.21. Other = 1.89.
        // Min amount is 1.0. 0.21 < 1.0. Should revert.
        (bool success, ) = address(feeVault).call{value: 2 ether}("");
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

        // Test setBurnShare
        vm.prank(newOwner);
        feeVault.setBurnShare(5000);
        assertEq(feeVault.burnShareBps(), 5000);

        vm.prank(newOwner);
        vm.expectRevert("FeeVault: invalid bps");
        feeVault.setBurnShare(10001);
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
        feeVault.setBurnShare(5000);

        vm.prank(user);
        vm.expectRevert("FeeVault: caller is not the owner");
        feeVault.withdrawOther(payable(user), 1);
    }
}
