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
    }

    function test_Receive() public {
        uint256 amount = 1 ether;
        (bool success, ) = address(burnCollector).call{value: amount}("");
        assertTrue(success, "Transfer failed");
        assertEq(address(burnCollector).balance, amount, "Balance mismatch");
    }

    function test_SendToCelestia() public {
        // Fund the collector with enough to meet minAmount (excluding fee which is added on top)
        // Actually, the check is address(this).balance >= minAmount.
        // If we send fee, it adds to balance.
        // Let's fund it with minAmount first.
        (bool success, ) = address(burnCollector).call{value: minAmount}("");
        require(success, "Funding failed");

        // Expect the call to the mock minter
        // Total sent = existing balance (minAmount) + fee sent in call
        uint256 totalAmount = minAmount + fee;

        vm.expectEmit(true, true, true, true, address(mockMinter));
        emit MockHypNativeMinter.TransferRemoteCalled(destination, recipient, totalAmount);

        // Expect the event from BurnCollector
        vm.expectEmit(true, true, true, true, address(burnCollector));
        emit BurnCollector.SentToCelestia(totalAmount, recipient, bytes32(uint256(1)));

        // Call as user with fee
        vm.prank(user);
        vm.deal(user, fee);
        burnCollector.sendToCelestia{value: fee}();

        assertEq(address(burnCollector).balance, 0, "Collector should be empty");
        assertEq(address(mockMinter).balance, totalAmount, "Minter should have received funds");
    }

    function test_SendToCelestia_InsufficientFee() public {
        vm.prank(user);
        vm.deal(user, fee);
        // Send less than fee
        vm.expectRevert("BurnCollector: insufficient fee");
        burnCollector.sendToCelestia{value: fee - 1}();
    }

    function test_SendToCelestia_BelowMinAmount() public {
        // Fund with less than minAmount
        uint256 amount = minAmount - 0.5 ether;
        (bool success, ) = address(burnCollector).call{value: amount}("");
        require(success, "Funding failed");

        // Even with fee, if logic checks total balance, we need to be careful.
        // Logic: require(address(this).balance >= minimumAmount)
        // If we send fee, balance increases.
        // Let's assume we want to test when TOTAL balance is still low.
        // If minAmount is 1 ether. Funding is 0.5. Fee is 0.1. Total 0.6 < 1.0.
        
        // Reset to clean state for clarity
        burnCollector = new BurnCollector(address(mockMinter), owner);
        burnCollector.setRecipient(destination, recipient);
        burnCollector.setMinimumAmount(1 ether);
        burnCollector.setCallFee(0.1 ether);

        (success, ) = address(burnCollector).call{value: 0.5 ether}("");
        require(success);

        vm.prank(user);
        vm.deal(user, 1 ether);
        vm.expectRevert("BurnCollector: minimum amount not met");
        burnCollector.sendToCelestia{value: 0.1 ether}();
    }

    function test_AdminFunctions() public {
        // Test setRecipient
        burnCollector.setRecipient(5678, bytes32(uint256(0xbeef)));
        assertEq(burnCollector.destinationDomain(), 5678);
        assertEq(burnCollector.recipientAddress(), bytes32(uint256(0xbeef)));

        // Test setMinimumAmount
        burnCollector.setMinimumAmount(5 ether);
        assertEq(burnCollector.minimumAmount(), 5 ether);

        // Test setCallFee
        burnCollector.setCallFee(1 ether);
        assertEq(burnCollector.callFee(), 1 ether);

        // Test transferOwnership
        address newOwner = address(0x2);
        burnCollector.transferOwnership(newOwner);
        assertEq(burnCollector.owner(), newOwner);

        // Old owner cannot call anymore
        vm.expectRevert("BurnCollector: caller is not the owner");
        burnCollector.setCallFee(2 ether);
    }

    function test_AdminAccessControl() public {
        vm.prank(user);
        vm.expectRevert("BurnCollector: caller is not the owner");
        burnCollector.setRecipient(1, bytes32(0));

        vm.prank(user);
        vm.expectRevert("BurnCollector: caller is not the owner");
        burnCollector.setMinimumAmount(1);

        vm.prank(user);
        vm.expectRevert("BurnCollector: caller is not the owner");
        burnCollector.setCallFee(1);

        vm.prank(user);
        vm.expectRevert("BurnCollector: caller is not the owner");
        burnCollector.transferOwnership(user);
    }
}
