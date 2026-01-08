// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test, console} from "forge-std/Test.sol";
import {AdminProxy} from "../src/AdminProxy.sol";
import {FeeVault} from "../src/FeeVault.sol";

/// @dev Mock contract to test AdminProxy execute functionality
contract MockTarget {
    uint256 public value;
    address public lastCaller;

    error CustomError(string message);

    function setValue(uint256 _value) external {
        value = _value;
        lastCaller = msg.sender;
    }

    function getValue() external view returns (uint256) {
        return value;
    }

    function revertWithMessage() external pure {
        revert("MockTarget: intentional revert");
    }

    function revertWithCustomError() external pure {
        revert CustomError("custom error");
    }

    function payableFunction() external payable {
        value = msg.value;
    }
}

/// @dev Mock mint precompile interface for testing
contract MockMintPrecompile {
    mapping(address => bool) public allowlist;
    address public admin;

    error NotAdmin();

    constructor(address _admin) {
        admin = _admin;
    }

    modifier onlyAdmin() {
        if (msg.sender != admin) revert NotAdmin();
        _;
    }

    function addToAllowList(address account) external onlyAdmin {
        allowlist[account] = true;
    }

    function removeFromAllowList(address account) external onlyAdmin {
        allowlist[account] = false;
    }
}

contract AdminProxyTest is Test {
    AdminProxy public proxy;
    MockTarget public target;
    MockMintPrecompile public mintPrecompile;

    address public alice = address(0x1);
    address public bob = address(0x2);
    address public multisig = address(0x3);

    event OwnershipTransferStarted(address indexed previousOwner, address indexed newOwner);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event Executed(address indexed target, bytes data, bytes result);

    function setUp() public {
        proxy = new AdminProxy();
        target = new MockTarget();
    }

    // ============ Ownership Tests ============

    function test_InitialOwnerIsZero() public view {
        assertEq(proxy.owner(), address(0));
    }

    function test_ClaimOwnership() public {
        vm.prank(alice);
        vm.expectEmit(true, true, false, false);
        emit OwnershipTransferred(address(0), alice);
        proxy.claimOwnership();

        assertEq(proxy.owner(), alice);
    }

    function test_ClaimOwnership_RevertWhenAlreadyClaimed() public {
        vm.prank(alice);
        proxy.claimOwnership();

        vm.prank(bob);
        vm.expectRevert(AdminProxy.NotOwner.selector);
        proxy.claimOwnership();
    }

    function test_TransferOwnership_TwoStep() public {
        // Alice claims ownership
        vm.prank(alice);
        proxy.claimOwnership();

        // Alice initiates transfer to bob
        vm.prank(alice);
        vm.expectEmit(true, true, false, false);
        emit OwnershipTransferStarted(alice, bob);
        proxy.transferOwnership(bob);

        assertEq(proxy.owner(), alice); // Still alice
        assertEq(proxy.pendingOwner(), bob);

        // Bob accepts
        vm.prank(bob);
        vm.expectEmit(true, true, false, false);
        emit OwnershipTransferred(alice, bob);
        proxy.acceptOwnership();

        assertEq(proxy.owner(), bob);
        assertEq(proxy.pendingOwner(), address(0));
    }

    function test_TransferOwnership_RevertZeroAddress() public {
        vm.prank(alice);
        proxy.claimOwnership();

        vm.prank(alice);
        vm.expectRevert(AdminProxy.ZeroAddress.selector);
        proxy.transferOwnership(address(0));
    }

    function test_AcceptOwnership_RevertNotPending() public {
        vm.prank(alice);
        proxy.claimOwnership();

        vm.prank(alice);
        proxy.transferOwnership(bob);

        // Charlie tries to accept
        address charlie = address(0x4);
        vm.prank(charlie);
        vm.expectRevert(AdminProxy.NotPendingOwner.selector);
        proxy.acceptOwnership();
    }

    function test_CancelTransfer() public {
        vm.prank(alice);
        proxy.claimOwnership();

        vm.prank(alice);
        proxy.transferOwnership(bob);
        assertEq(proxy.pendingOwner(), bob);

        vm.prank(alice);
        proxy.cancelTransfer();
        assertEq(proxy.pendingOwner(), address(0));

        // Bob can no longer accept
        vm.prank(bob);
        vm.expectRevert(AdminProxy.NotPendingOwner.selector);
        proxy.acceptOwnership();
    }

    function test_TransferOwnership_RevertNotOwner() public {
        vm.prank(alice);
        proxy.claimOwnership();

        vm.prank(bob);
        vm.expectRevert(AdminProxy.NotOwner.selector);
        proxy.transferOwnership(bob);
    }

    // ============ Execute Tests ============

    function test_Execute() public {
        vm.prank(alice);
        proxy.claimOwnership();

        bytes memory data = abi.encodeCall(MockTarget.setValue, (42));

        vm.prank(alice);
        vm.expectEmit(true, false, false, false);
        emit Executed(address(target), data, "");
        proxy.execute(address(target), data);

        assertEq(target.value(), 42);
        assertEq(target.lastCaller(), address(proxy)); // Proxy is the caller
    }

    function test_Execute_ReturnsData() public {
        vm.prank(alice);
        proxy.claimOwnership();

        // First set a value
        vm.prank(alice);
        proxy.execute(address(target), abi.encodeCall(MockTarget.setValue, (123)));

        // Then get it
        vm.prank(alice);
        bytes memory result = proxy.execute(address(target), abi.encodeCall(MockTarget.getValue, ()));

        uint256 decoded = abi.decode(result, (uint256));
        assertEq(decoded, 123);
    }

    function test_Execute_RevertNotOwner() public {
        vm.prank(alice);
        proxy.claimOwnership();

        vm.prank(bob);
        vm.expectRevert(AdminProxy.NotOwner.selector);
        proxy.execute(address(target), abi.encodeCall(MockTarget.setValue, (42)));
    }

    function test_Execute_PropagatesRevert() public {
        vm.prank(alice);
        proxy.claimOwnership();

        // The revert data is ABI-encoded as Error(string), not raw bytes
        bytes memory expectedRevertData = abi.encodeWithSignature("Error(string)", "MockTarget: intentional revert");

        vm.prank(alice);
        vm.expectRevert(abi.encodeWithSelector(AdminProxy.CallFailed.selector, expectedRevertData));
        proxy.execute(address(target), abi.encodeCall(MockTarget.revertWithMessage, ()));
    }

    // ============ ExecuteBatch Tests ============

    function test_ExecuteBatch() public {
        vm.prank(alice);
        proxy.claimOwnership();

        MockTarget target2 = new MockTarget();

        address[] memory targets = new address[](2);
        targets[0] = address(target);
        targets[1] = address(target2);

        bytes[] memory datas = new bytes[](2);
        datas[0] = abi.encodeCall(MockTarget.setValue, (100));
        datas[1] = abi.encodeCall(MockTarget.setValue, (200));

        vm.prank(alice);
        proxy.executeBatch(targets, datas);

        assertEq(target.value(), 100);
        assertEq(target2.value(), 200);
    }

    function test_ExecuteBatch_RevertLengthMismatch() public {
        vm.prank(alice);
        proxy.claimOwnership();

        address[] memory targets = new address[](2);
        bytes[] memory datas = new bytes[](1);

        vm.prank(alice);
        vm.expectRevert(AdminProxy.LengthMismatch.selector);
        proxy.executeBatch(targets, datas);
    }

    function test_ExecuteBatch_RevertOnAnyFailure() public {
        vm.prank(alice);
        proxy.claimOwnership();

        address[] memory targets = new address[](2);
        targets[0] = address(target);
        targets[1] = address(target);

        bytes[] memory datas = new bytes[](2);
        datas[0] = abi.encodeCall(MockTarget.setValue, (100));
        datas[1] = abi.encodeCall(MockTarget.revertWithMessage, ());

        // The revert data is ABI-encoded as Error(string), not raw bytes
        bytes memory expectedRevertData = abi.encodeWithSignature("Error(string)", "MockTarget: intentional revert");

        vm.prank(alice);
        vm.expectRevert(abi.encodeWithSelector(AdminProxy.CallFailed.selector, expectedRevertData));
        proxy.executeBatch(targets, datas);
    }

    // ============ ExecuteWithValue Tests ============

    function test_ExecuteWithValue() public {
        vm.prank(alice);
        proxy.claimOwnership();

        // Fund the proxy
        vm.deal(address(proxy), 1 ether);

        vm.prank(alice);
        proxy.executeWithValue(address(target), abi.encodeCall(MockTarget.payableFunction, ()), 0.5 ether);

        assertEq(target.value(), 0.5 ether);
        assertEq(address(proxy).balance, 0.5 ether);
    }

    function test_ReceiveEth() public {
        (bool success,) = address(proxy).call{value: 1 ether}("");
        assertTrue(success);
        assertEq(address(proxy).balance, 1 ether);
    }

    // ============ Integration Tests ============

    function test_Integration_ProxyAsMintPrecompileAdmin() public {
        // Deploy mint precompile with proxy as admin
        mintPrecompile = new MockMintPrecompile(address(proxy));

        // Alice claims proxy ownership
        vm.prank(alice);
        proxy.claimOwnership();

        // Alice uses proxy to add bob to allowlist
        vm.prank(alice);
        proxy.execute(address(mintPrecompile), abi.encodeCall(MockMintPrecompile.addToAllowList, (bob)));

        assertTrue(mintPrecompile.allowlist(bob));

        // Direct call fails (alice is not admin, proxy is)
        vm.prank(alice);
        vm.expectRevert(MockMintPrecompile.NotAdmin.selector);
        mintPrecompile.addToAllowList(address(0x5));
    }

    function test_Integration_TransferToMultisig() public {
        // Simulate genesis -> multisig flow
        mintPrecompile = new MockMintPrecompile(address(proxy));

        // 1. Alice (EOA) claims ownership post-genesis
        vm.prank(alice);
        proxy.claimOwnership();

        // 2. Alice does some admin work
        vm.prank(alice);
        proxy.execute(address(mintPrecompile), abi.encodeCall(MockMintPrecompile.addToAllowList, (bob)));

        // 3. Multisig is deployed (simulated)
        // 4. Alice transfers to multisig
        vm.prank(alice);
        proxy.transferOwnership(multisig);

        // 5. Multisig accepts
        vm.prank(multisig);
        proxy.acceptOwnership();

        assertEq(proxy.owner(), multisig);

        // 6. Multisig can now admin
        vm.prank(multisig);
        proxy.execute(address(mintPrecompile), abi.encodeCall(MockMintPrecompile.removeFromAllowList, (bob)));

        assertFalse(mintPrecompile.allowlist(bob));

        // 7. Alice can no longer admin
        vm.prank(alice);
        vm.expectRevert(AdminProxy.NotOwner.selector);
        proxy.execute(address(mintPrecompile), abi.encodeCall(MockMintPrecompile.addToAllowList, (alice)));
    }

    function test_Integration_ProxyAsFeeVaultOwner() public {
        // Deploy FeeVault with proxy as owner
        FeeVault vault = new FeeVault(
            address(proxy), // proxy is owner
            1234,
            bytes32(uint256(0xbeef)),
            1 ether,
            0.1 ether,
            10000,
            address(0x99)
        );

        // Alice claims proxy ownership
        vm.prank(alice);
        proxy.claimOwnership();

        // Alice uses proxy to update FeeVault config
        vm.prank(alice);
        proxy.execute(address(vault), abi.encodeCall(FeeVault.setMinimumAmount, (2 ether)));

        assertEq(vault.minimumAmount(), 2 ether);

        // Direct call fails
        vm.prank(alice);
        vm.expectRevert("FeeVault: caller is not the owner");
        vault.setMinimumAmount(3 ether);
    }

    function test_Integration_BatchAllowlistUpdates() public {
        mintPrecompile = new MockMintPrecompile(address(proxy));

        vm.prank(alice);
        proxy.claimOwnership();

        // Batch add multiple addresses to allowlist
        address[] memory targets = new address[](3);
        bytes[] memory datas = new bytes[](3);

        address user1 = address(0x10);
        address user2 = address(0x11);
        address user3 = address(0x12);

        targets[0] = address(mintPrecompile);
        targets[1] = address(mintPrecompile);
        targets[2] = address(mintPrecompile);

        datas[0] = abi.encodeCall(MockMintPrecompile.addToAllowList, (user1));
        datas[1] = abi.encodeCall(MockMintPrecompile.addToAllowList, (user2));
        datas[2] = abi.encodeCall(MockMintPrecompile.addToAllowList, (user3));

        vm.prank(alice);
        proxy.executeBatch(targets, datas);

        assertTrue(mintPrecompile.allowlist(user1));
        assertTrue(mintPrecompile.allowlist(user2));
        assertTrue(mintPrecompile.allowlist(user3));
    }
}
