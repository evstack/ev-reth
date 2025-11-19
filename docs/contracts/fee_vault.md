# FeeVault Design & Use Case

## Overview
The `FeeVault` is a specialized smart contract designed to accumulate native tokens (ETH or gas tokens) and periodically bridge them to a specific destination chain (e.g., Celestia) via the Hyperlane protocol.

## Use Case
This contract serves as a **fee sink** and **bridging mechanism** for a rollup or chain that wants to redirect collected fees (e.g., EIP-1559 base fees) to another ecosystem.

1. **Fee Accumulation**: The contract receives funds from:
    - **Base Fee Redirect**: The chain's execution layer (e.g., `ev-revm`) can be configured to direct burned base fees directly to this contract's address.
    - **Direct Transfers**: Anyone can send native tokens to the contract via the `receive()` function.

2. **Bridging to Celestia**: Once sufficient funds have accumulated, any user can trigger the `sendToCelestia()` function. This converts the native balance into a cross-chain message that mints tokens on the destination chain (Celestia) via the `HypNativeMinter`.

## Architecture

### Core Components
- **HypNativeMinter Integration**: The contract interacts with a Hyperlane `HypNativeMinter` to handle the cross-chain transfer logic.
- **Admin Controls**: An `owner` manages critical parameters to ensure security and flexibility.

### Key Features
- **Stored Recipient**: The destination domain (Chain ID) and recipient address are stored in the contract state, preventing callers from redirecting funds to arbitrary addresses.
- **Minimum Threshold**: A `minimumAmount` ensures that bridging only occurs when it is economically viable (avoiding dust transfers).
- **Caller Incentive/Fee**: A `callFee` is required to trigger the bridge function. This fee is added to the total bridged amount, effectively making the caller pay for the privilege (or potentially subsidizing it if the protocol design changes). *Note: Currently, the caller pays the fee, which is added to the pot.*

## Workflow

1. **Accumulation Phase**:
   - Block producers/Execution layer sends base fees to `FeeVault`.
   - Users/Contracts send ETH to `FeeVault`.

2. **Trigger Phase**:
   - A keeper or user notices the balance exceeds `minimumAmount`.
   - They call `sendToCelestia{value: callFee}()`.
   - The contract checks:
     - `msg.value >= callFee`
     - `address(this).balance >= minimumAmount`

3. **Execution Phase**:
   - The contract calls `hypNativeMinter.transferRemote`.
   - The entire balance (accumulated funds + `callFee`) is sent as `msg.value` to the minter.
   - The minter burns/locks the tokens and sends a message to the destination chain.
   - `SentToCelestia` event is emitted.

## Configuration Parameters
| Parameter | Description | Managed By |
|-----------|-------------|------------|
| `destinationDomain` | Hyperlane domain ID of the target chain (e.g., Celestia). | Owner |
| `recipientAddress` | Address on the target chain to receive funds. | Owner |
| `minimumAmount` | Minimum balance required to trigger a bridge tx. | Owner |
| `callFee` | Fee required from the caller to execute the function. | Owner |
