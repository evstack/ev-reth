# FeeVault Design & Use Case

## Overview
The `FeeVault` is a specialized smart contract designed to accumulate native tokens (ETH or gas tokens) and automatically split them between bridging to a specific destination chain (e.g., Celestia) and sending to a secondary recipient.

## Use Case
This contract serves as a **fee sink** and **bridging mechanism** for a rollup or chain that wants to redirect collected fees (e.g., EIP-1559 base fees) to another ecosystem while retaining a portion for other purposes (e.g., developer rewards, treasury).

1. **Fee Accumulation**: The contract receives funds from:
    - **Base Fee Redirect**: The chain's execution layer (e.g., `ev-revm`) can be configured to direct burned base fees directly to this contract's address.
    - **Direct Transfers**: Anyone can send native tokens to the contract via the `receive()` function.

2. **Splitting & Bridging**: Once sufficient funds have accumulated, any user can trigger the `sendToCelestia()` function. This splits the funds based on a configured percentage:
    - **Bridge Share**: Sent to the destination chain (Celestia) via the `HypNativeMinter`.
    - **Other Share**: Immediately transferred to a configured `otherRecipient` address.

## Architecture

### Core Components
- **HypNativeMinter Integration**: The contract interacts with a Hyperlane `HypNativeMinter` to handle the cross-chain transfer logic.
- **Admin Controls**: An `owner` manages critical parameters to ensure security and flexibility.

### Key Features
- **Automatic Splitting**: Funds are split automatically upon calling `sendToCelestia`. No manual withdrawal is required for the secondary recipient.
- **Stored Recipient**: The destination domain (Chain ID) and recipient address are stored in the contract state.
- **Minimum Threshold**: A `minimumAmount` ensures that bridging only occurs when it is economically viable.
- **Caller Incentive/Fee**: A `callFee` is required to trigger the bridge function.

## Workflow

1. **Accumulation Phase**:
   - Block producers/Execution layer sends base fees to `FeeVault`.
   - Users/Contracts send ETH to `FeeVault`.

2. **Trigger Phase**:
   - A keeper or user notices the bridge portion exceeds `minimumAmount`.
   - They call `sendToCelestia{value: callFee}()`.
   - The contract checks:
     - `msg.value >= callFee`
     - `bridgeAmount >= minimumAmount`

3. **Execution Phase**:
   - The contract calculates the split based on `bridgeShareBps`.
   - **Other Share**: Transferred immediately to `otherRecipient`.
   - **Bridge Share**: Bridged to Celestia via `hypNativeMinter.transferRemote`.
   - `SentToCelestia` and `FundsSplit` events are emitted.

## Configuration Parameters
| Parameter | Description | Managed By |
|-----------|-------------|------------|
| `destinationDomain` | Hyperlane domain ID of the target chain (e.g., Celestia). | Owner |
| `recipientAddress` | Address on the target chain to receive funds. | Owner |
| `minimumAmount` | Minimum bridge amount required to trigger a bridge tx. | Owner |
| `callFee` | Fee required from the caller to execute the function. | Owner |
| `bridgeShareBps` | Basis points (0-10000) determining the % of funds to bridge. | Owner |
| `otherRecipient` | Address to receive the non-bridged portion of funds. | Owner |
