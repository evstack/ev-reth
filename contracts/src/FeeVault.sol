// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract FeeVault {
    address public owner;
    address public bridgeRecipient;
    address public otherRecipient;
    uint256 public minimumAmount;
    uint256 public callFee;
    uint256 public bridgeShareBps; // Basis points (0-10000) for bridge share

    event FundsDistributed(uint256 total, uint256 bridgeAmount, uint256 otherAmount);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event BridgeRecipientUpdated(address bridgeRecipient);
    event MinimumAmountUpdated(uint256 minimumAmount);
    event CallFeeUpdated(uint256 callFee);
    event BridgeShareUpdated(uint256 bridgeShareBps);
    event OtherRecipientUpdated(address otherRecipient);

    modifier onlyOwner() {
        require(msg.sender == owner, "FeeVault: caller is not the owner");
        _;
    }

    constructor(
        address _owner,
        uint256 _minimumAmount,
        uint256 _callFee,
        uint256 _bridgeShareBps,
        address _otherRecipient
    ) {
        require(_owner != address(0), "FeeVault: owner is the zero address");
        require(_bridgeShareBps <= 10000, "FeeVault: invalid bps");

        owner = _owner;
        minimumAmount = _minimumAmount;
        callFee = _callFee;
        bridgeShareBps = _bridgeShareBps == 0 ? 10000 : _bridgeShareBps;
        otherRecipient = _otherRecipient;

        emit OwnershipTransferred(address(0), _owner);
    }

    receive() external payable {}

    function distribute() external payable {
        require(bridgeRecipient != address(0), "FeeVault: bridge recipient not set");
        require(msg.value >= callFee, "FeeVault: insufficient fee");

        uint256 currentBalance = address(this).balance;

        // Calculate split
        uint256 bridgeAmount = (currentBalance * bridgeShareBps) / 10000;
        uint256 otherAmount = currentBalance - bridgeAmount;

        require(bridgeAmount >= minimumAmount, "FeeVault: minimum amount not met");

        emit FundsDistributed(currentBalance, bridgeAmount, otherAmount);

        // Send other amount if any
        if (otherAmount > 0) {
            require(otherRecipient != address(0), "FeeVault: other recipient not set");
            (bool sent,) = otherRecipient.call{value: otherAmount}("");
            require(sent, "FeeVault: transfer failed");
        }

        // Send bridge amount
        (bool success,) = bridgeRecipient.call{value: bridgeAmount}("");
        require(success, "FeeVault: bridge transfer failed");
    }

    // Admin functions

    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "FeeVault: new owner is the zero address");
        emit OwnershipTransferred(owner, newOwner);
        owner = newOwner;
    }

    function setBridgeRecipient(address _bridgeRecipient) external onlyOwner {
        require(_bridgeRecipient != address(0), "FeeVault: zero address");
        bridgeRecipient = _bridgeRecipient;
        emit BridgeRecipientUpdated(_bridgeRecipient);
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

    function getConfig()
        external
        view
        returns (
            address _owner,
            address _bridgeRecipient,
            address _otherRecipient,
            uint256 _minimumAmount,
            uint256 _callFee,
            uint256 _bridgeShareBps
        )
    {
        return (owner, bridgeRecipient, otherRecipient, minimumAmount, callFee, bridgeShareBps);
    }
}
