// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

interface IHypNativeMinter {
    function transferRemote(uint32 _destination, bytes32 _recipient, uint256 _amount)
        external
        payable
        returns (bytes32 messageId);
}

contract FeeVault {
    IHypNativeMinter public hypNativeMinter;

    address public owner;
    uint32 public destinationDomain;
    bytes32 public recipientAddress;
    uint256 public minimumAmount;
    uint256 public callFee;

    // Split accounting
    address public otherRecipient;
    uint256 public bridgeShareBps; // Basis points (0-10000) for bridge share

    event SentToCelestia(uint256 amount, bytes32 recipient, bytes32 messageId);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event HypNativeMinterUpdated(address hypNativeMinter);
    event RecipientUpdated(uint32 destinationDomain, bytes32 recipientAddress);
    event MinimumAmountUpdated(uint256 minimumAmount);
    event CallFeeUpdated(uint256 callFee);
    event BridgeShareUpdated(uint256 bridgeShareBps);
    event OtherRecipientUpdated(address otherRecipient);
    event FundsSplit(uint256 totalNew, uint256 bridgeAmount, uint256 otherAmount);

    modifier onlyOwner() {
        require(msg.sender == owner, "FeeVault: caller is not the owner");
        _;
    }

    constructor(address _owner) {
        require(_owner != address(0), "FeeVault: owner is the zero address");
        owner = _owner;
        bridgeShareBps = 10000; // Default to 100% bridge
        emit OwnershipTransferred(address(0), _owner);
    }

    receive() external payable {}

    function sendToCelestia() external payable {
        require(address(hypNativeMinter) != address(0), "FeeVault: minter not set");
        require(msg.value >= callFee, "FeeVault: insufficient fee");

        uint256 currentBalance = address(this).balance;

        // Calculate split
        uint256 bridgeAmount = (currentBalance * bridgeShareBps) / 10000;
        uint256 otherAmount = currentBalance - bridgeAmount;

        require(bridgeAmount >= minimumAmount, "FeeVault: minimum amount not met");

        emit FundsSplit(currentBalance, bridgeAmount, otherAmount);

        // Send other amount if any
        if (otherAmount > 0) {
            require(otherRecipient != address(0), "FeeVault: other recipient not set");
            (bool success,) = otherRecipient.call{value: otherAmount}("");
            require(success, "FeeVault: transfer failed");
        }

        // Bridge the bridge amount
        bytes32 messageId =
            hypNativeMinter.transferRemote{value: bridgeAmount}(destinationDomain, recipientAddress, bridgeAmount);

        emit SentToCelestia(bridgeAmount, recipientAddress, messageId);
    }

    // Admin functions

    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "FeeVault: new owner is the zero address");
        emit OwnershipTransferred(owner, newOwner);
        owner = newOwner;
    }

    function setRecipient(uint32 _destinationDomain, bytes32 _recipientAddress) external onlyOwner {
        destinationDomain = _destinationDomain;
        recipientAddress = _recipientAddress;
        emit RecipientUpdated(_destinationDomain, _recipientAddress);
    }

    function setMinimumAmount(uint256 _minimumAmount) external onlyOwner {
        minimumAmount = _minimumAmount;
        emit MinimumAmountUpdated(_minimumAmount);
    }

    function setCallFee(uint256 _callFee) external onlyOwner {
        callFee = _callFee;
        emit CallFeeUpdated(_callFee);
    }

    function setBridgeShare(uint256 _bridgeShareBps) external onlyOwner {
        require(_bridgeShareBps <= 10000, "FeeVault: invalid bps");
        bridgeShareBps = _bridgeShareBps;
        emit BridgeShareUpdated(_bridgeShareBps);
    }

    function setOtherRecipient(address _otherRecipient) external onlyOwner {
        require(_otherRecipient != address(0), "FeeVault: zero address");
        otherRecipient = _otherRecipient;
        emit OtherRecipientUpdated(_otherRecipient);
    }

    function setHypNativeMinter(address _hypNativeMinter) external onlyOwner {
        require(_hypNativeMinter != address(0), "FeeVault: zero address");
        hypNativeMinter = IHypNativeMinter(_hypNativeMinter);
        emit HypNativeMinterUpdated(_hypNativeMinter);
    }
}
