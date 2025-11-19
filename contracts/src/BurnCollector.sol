// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

interface IHypNativeMinter {
    function transferRemote(
        uint32 _destination,
        bytes32 _recipient,
        uint256 _amount
    ) external payable returns (bytes32 messageId);
}

contract BurnCollector {
    IHypNativeMinter public immutable hypNativeMinter;

    address public owner;
    uint32 public destinationDomain;
    bytes32 public recipientAddress;
    uint256 public minimumAmount;
    uint256 public callFee;

    // Split accounting
    uint256 public otherBucketBalance;
    uint256 public burnShareBps; // Basis points (0-10000) for burn share

    event SentToCelestia(uint256 amount, bytes32 recipient, bytes32 messageId);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event RecipientUpdated(uint32 destinationDomain, bytes32 recipientAddress);
    event MinimumAmountUpdated(uint256 minimumAmount);
    event CallFeeUpdated(uint256 callFee);
    event BurnShareUpdated(uint256 burnShareBps);
    event OtherWithdrawn(address indexed recipient, uint256 amount);
    event FundsSplit(uint256 totalNew, uint256 burnAmount, uint256 otherAmount);

    modifier onlyOwner() {
        require(msg.sender == owner, "BurnCollector: caller is not the owner");
        _;
    }

    constructor(address _hypNativeMinter, address _owner) {
        hypNativeMinter = IHypNativeMinter(_hypNativeMinter);
        owner = _owner;
        burnShareBps = 10000; // Default to 100% burn
        emit OwnershipTransferred(address(0), _owner);
    }

    receive() external payable {}

    function sendToCelestia() external payable {
        require(msg.value >= callFee, "BurnCollector: insufficient fee");
        
        // Calculate new funds available for splitting
        // Total Balance - Already Allocated to Other Bucket
        uint256 currentBalance = address(this).balance;
        require(currentBalance >= otherBucketBalance, "BurnCollector: accounting error");
        
        uint256 newFunds = currentBalance - otherBucketBalance;
        
        // Calculate split
        uint256 burnAmount = (newFunds * burnShareBps) / 10000;
        uint256 otherAmount = newFunds - burnAmount;

        require(burnAmount >= minimumAmount, "BurnCollector: minimum amount not met");

        // Update accounting
        otherBucketBalance += otherAmount;
        emit FundsSplit(newFunds, burnAmount, otherAmount);

        // Bridge the burn amount
        bytes32 messageId = hypNativeMinter.transferRemote{value: burnAmount}(
            destinationDomain,
            recipientAddress,
            burnAmount
        );

        emit SentToCelestia(burnAmount, recipientAddress, messageId);
    }

    // Admin functions

    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "BurnCollector: new owner is the zero address");
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

    function setBurnShare(uint256 _burnShareBps) external onlyOwner {
        require(_burnShareBps <= 10000, "BurnCollector: invalid bps");
        burnShareBps = _burnShareBps;
        emit BurnShareUpdated(_burnShareBps);
    }

    function withdrawOther(address payable _recipient, uint256 _amount) external onlyOwner {
        require(_amount <= otherBucketBalance, "BurnCollector: insufficient other balance");
        otherBucketBalance -= _amount;
        
        (bool success, ) = _recipient.call{value: _amount}("");
        require(success, "BurnCollector: transfer failed");
        
        emit OtherWithdrawn(_recipient, _amount);
    }
}
