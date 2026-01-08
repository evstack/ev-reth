// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title AdminProxy
/// @notice A proxy contract for managing admin rights to precompiles and other contracts.
/// @dev Deployed at genesis with zero owner, allowing first-come claim. Supports two-step
/// ownership transfer for safe handoff to multisigs or other governance contracts.
///
/// This contract solves the bootstrap problem where admin addresses (e.g., multisigs)
/// are not known at genesis time. The proxy is set as admin in the chainspec, and
/// ownership can be claimed and transferred post-genesis.
///
/// Usage:
/// 1. Deploy at genesis with zero owner (via genesis alloc)
/// 2. Set proxy address as `mintAdmin` in chainspec and as FeeVault owner
/// 3. Post-genesis: call claimOwnership() to become initial owner
/// 4. Deploy multisig, then transferOwnership() -> acceptOwnership() to hand off
contract AdminProxy {
    /// @notice Current owner of the proxy
    address public owner;

    /// @notice Pending owner for two-step transfer
    address public pendingOwner;

    /// @notice Emitted when ownership transfer is initiated
    event OwnershipTransferStarted(address indexed previousOwner, address indexed newOwner);

    /// @notice Emitted when ownership transfer is completed
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);

    /// @notice Emitted when a call is executed through the proxy
    event Executed(address indexed target, bytes data, bytes result);

    /// @notice Thrown when caller is not the owner
    error NotOwner();

    /// @notice Thrown when caller is not the pending owner
    error NotPendingOwner();

    /// @notice Thrown when a call to target contract fails
    error CallFailed(bytes reason);

    /// @notice Thrown when array lengths don't match in batch operations
    error LengthMismatch();

    /// @notice Thrown when trying to set zero address as pending owner
    error ZeroAddress();

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    /// @notice Initialize with zero owner - first caller can claim ownership
    constructor() {
        owner = address(0);
    }

    /// @notice Claim ownership when owner is zero (genesis bootstrap)
    /// @dev Can only be called once, when owner is address(0)
    function claimOwnership() external {
        if (owner != address(0)) revert NotOwner();
        owner = msg.sender;
        emit OwnershipTransferred(address(0), msg.sender);
    }

    /// @notice Start two-step ownership transfer
    /// @param newOwner Address of the new owner (e.g., multisig)
    function transferOwnership(address newOwner) external onlyOwner {
        if (newOwner == address(0)) revert ZeroAddress();
        pendingOwner = newOwner;
        emit OwnershipTransferStarted(owner, newOwner);
    }

    /// @notice Complete two-step ownership transfer
    /// @dev Must be called by the pending owner
    function acceptOwnership() external {
        if (msg.sender != pendingOwner) revert NotPendingOwner();
        emit OwnershipTransferred(owner, msg.sender);
        owner = msg.sender;
        pendingOwner = address(0);
    }

    /// @notice Cancel pending ownership transfer
    function cancelTransfer() external onlyOwner {
        pendingOwner = address(0);
    }

    /// @notice Execute a call to any target contract
    /// @param target Address of the contract to call
    /// @param data Calldata to send
    /// @return result The return data from the call
    /// @dev Use this to call admin functions on FeeVault, precompiles, etc.
    ///
    /// Example - Add address to mint precompile allowlist:
    ///   execute(MINT_PRECOMPILE, abi.encodeCall(IMintPrecompile.addToAllowList, (account)))
    ///
    /// Example - Transfer FeeVault ownership:
    ///   execute(feeVault, abi.encodeCall(FeeVault.transferOwnership, (newOwner)))
    function execute(address target, bytes calldata data) external onlyOwner returns (bytes memory result) {
        (bool success, bytes memory returnData) = target.call(data);
        if (!success) {
            revert CallFailed(returnData);
        }
        emit Executed(target, data, returnData);
        return returnData;
    }

    /// @notice Execute multiple calls in a single transaction
    /// @param targets Array of contract addresses to call
    /// @param datas Array of calldata for each call
    /// @return results Array of return data from each call
    /// @dev Useful for batch operations like adding multiple addresses to allowlist
    function executeBatch(address[] calldata targets, bytes[] calldata datas)
        external
        onlyOwner
        returns (bytes[] memory results)
    {
        if (targets.length != datas.length) revert LengthMismatch();

        results = new bytes[](targets.length);
        for (uint256 i = 0; i < targets.length; i++) {
            (bool success, bytes memory returnData) = targets[i].call(datas[i]);
            if (!success) {
                revert CallFailed(returnData);
            }
            emit Executed(targets[i], datas[i], returnData);
            results[i] = returnData;
        }
    }

    /// @notice Execute a call with ETH value
    /// @param target Address of the contract to call
    /// @param data Calldata to send
    /// @param value Amount of ETH to send
    /// @return result The return data from the call
    function executeWithValue(address target, bytes calldata data, uint256 value)
        external
        onlyOwner
        returns (bytes memory result)
    {
        (bool success, bytes memory returnData) = target.call{value: value}(data);
        if (!success) {
            revert CallFailed(returnData);
        }
        emit Executed(target, data, returnData);
        return returnData;
    }

    /// @notice Receive ETH (needed for executeWithValue)
    receive() external payable {}
}
