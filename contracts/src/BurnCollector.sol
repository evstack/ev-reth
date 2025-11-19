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

    event SentToCelestia(uint256 amount, bytes32 recipient, bytes32 messageId);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event RecipientUpdated(uint32 destinationDomain, bytes32 recipientAddress);
    event MinimumAmountUpdated(uint256 minimumAmount);
    event CallFeeUpdated(uint256 callFee);

    modifier onlyOwner() {
        require(msg.sender == owner, "BurnCollector: caller is not the owner");
        _;
    }

    constructor(address _hypNativeMinter, address _owner) {
        hypNativeMinter = IHypNativeMinter(_hypNativeMinter);
        owner = _owner;
        emit OwnershipTransferred(address(0), _owner);
    }

    receive() external payable {}

    function sendToCelestia() external payable {
        require(msg.value >= callFee, "BurnCollector: insufficient fee");
        
        uint256 balance = address(this).balance;
        require(balance >= minimumAmount, "BurnCollector: minimum amount not met");

        bytes32 messageId = hypNativeMinter.transferRemote{value: balance}(
            destinationDomain,
            recipientAddress,
            balance
        );

        emit SentToCelestia(balance, recipientAddress, messageId);
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
}
