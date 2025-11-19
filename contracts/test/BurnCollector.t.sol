// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test, console} from "forge-std/Test.sol";
import {BurnCollector} from "../src/BurnCollector.sol";

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

contract BurnCollectorTest is Test {
    BurnCollector public burnCollector;
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
        burnCollector = new BurnCollector(address(mockMinter), owner);

        // Configure contract
        burnCollector.setRecipient(destination, recipient);
        burnCollector.setMinimumAmount(minAmount);
        burnCollector.setCallFee(fee);
        // Default burn share is 10000 (100%)
    }

    function test_Receive() public {
        uint256 amount = 1 ether;
        (bool success, ) = address(burnCollector).call{value: amount}("");
        assertTrue(success, "Transfer failed");
        assertEq(address(burnCollector).balance, amount, "Balance mismatch");
    }

    function test_SendToCelestia_100PercentBurn() public {
        // Fund with minAmount
        (bool success, ) = address(burnCollector).call{value: minAmount}("");
        require(success);

        uint256 totalAmount = minAmount + fee;

        vm.expectEmit(true, true, true, true, address(mockMinter));
        emit MockHypNativeMinter.TransferRemoteCalled(destination, recipient, totalAmount);

        vm.prank(user);
        vm.deal(user, fee);
        burnCollector.sendToCelestia{value: fee}();

        assertEq(address(burnCollector).balance, 0, "Collector should be empty");
        assertEq(burnCollector.otherBucketBalance(), 0, "Other bucket should be empty");
    }

    function test_SendToCelestia_Split5050() public {
        // Set split to 50%
        burnCollector.setBurnShare(5000);

        // Fund with 2 ether. 
        // Fee is 0.1 ether.
        // Total new funds = 2.1 ether.
        // Burn = 1.05 ether. Other = 1.05 ether.
        // Min amount is 1 ether, so 1.05 >= 1.0 is OK.
        uint256 fundAmount = 2 ether;
        (bool success, ) = address(burnCollector).call{value: fundAmount}("");
        require(success);

        uint256 totalNew = fundAmount + fee;
        uint256 expectedBurn = totalNew / 2;
        uint256 expectedOther = totalNew - expectedBurn;

        vm.expectEmit(true, true, true, true, address(mockMinter));
        emit MockHypNativeMinter.TransferRemoteCalled(destination, recipient, expectedBurn);

        vm.prank(user);
        vm.deal(user, fee);
        burnCollector.sendToCelestia{value: fee}();

        assertEq(address(burnCollector).balance, expectedOther, "Collector should hold other funds");
        assertEq(burnCollector.otherBucketBalance(), expectedOther, "Other bucket accounting incorrect");
    }

    function test_SendToCelestia_AccumulateOther() public {
        burnCollector.setBurnShare(5000); // 50%

        // First call: 2 ether + 0.1 fee = 2.1 total. 1.05 burn, 1.05 other.
        (bool success, ) = address(burnCollector).call{value: 2 ether}("");
        require(success);
        
        vm.prank(user);
        vm.deal(user, fee);
        burnCollector.sendToCelestia{value: fee}();

        uint256 firstOther = 1.05 ether;
        assertEq(burnCollector.otherBucketBalance(), firstOther);

        // Second call: 2 ether + 0.1 fee = 2.1 total NEW funds.
        // Previous balance: 1.05. New balance before split: 1.05 + 2.1 = 3.15.
        // Logic: newFunds = balance (3.15) - otherBucket (1.05) = 2.1. Correct.
        (success, ) = address(burnCollector).call{value: 2 ether}("");
        require(success);

        vm.prank(user);
        vm.deal(user, fee);
        burnCollector.sendToCelestia{value: fee}();

        uint256 secondOther = 1.05 ether;
        assertEq(burnCollector.otherBucketBalance(), firstOther + secondOther);
        assertEq(address(burnCollector).balance, firstOther + secondOther);
    }

    function test_WithdrawOther() public {
        burnCollector.setBurnShare(5000);
        (bool success, ) = address(burnCollector).call{value: 2 ether}("");
        require(success);

        vm.prank(user);
        vm.deal(user, fee);
        burnCollector.sendToCelestia{value: fee}();

        uint256 otherAmount = 1.05 ether;
        assertEq(burnCollector.otherBucketBalance(), otherAmount);

        // Withdraw half
        uint256 withdrawAmount = 0.5 ether;
        address payable recipientAddr = payable(address(0x99));
        
        burnCollector.withdrawOther(recipientAddr, withdrawAmount);

        assertEq(burnCollector.otherBucketBalance(), otherAmount - withdrawAmount);
        assertEq(recipientAddr.balance, withdrawAmount);
        assertEq(address(burnCollector).balance, otherAmount - withdrawAmount);
    }

    function test_WithdrawOther_Insufficient() public {
        burnCollector.setBurnShare(5000);
        (bool success, ) = address(burnCollector).call{value: 2 ether}("");
        require(success);
        
        vm.prank(user);
        vm.deal(user, fee);
        burnCollector.sendToCelestia{value: fee}();

        uint256 otherAmount = 1.05 ether;
        
        vm.expectRevert("BurnCollector: insufficient other balance");
        burnCollector.withdrawOther(payable(owner), otherAmount + 1);
    }

    function test_SendToCelestia_BelowMinAmount_AfterSplit() public {
        burnCollector.setBurnShare(1000); // 10% burn
        
        // Fund with 2 ether. Total 2.1.
        // Burn = 0.21. Other = 1.89.
        // Min amount is 1.0. 0.21 < 1.0. Should revert.
        (bool success, ) = address(burnCollector).call{value: 2 ether}("");
        require(success);

        vm.prank(user);
        vm.deal(user, fee);
        vm.expectRevert("BurnCollector: minimum amount not met");
        burnCollector.sendToCelestia{value: fee}();
    }

    function test_AdminFunctions() public {
        burnCollector.setBurnShare(5000);
        assertEq(burnCollector.burnShareBps(), 5000);

        vm.expectRevert("BurnCollector: invalid bps");
        burnCollector.setBurnShare(10001);
    }
}
