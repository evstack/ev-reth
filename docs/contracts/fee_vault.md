# FeeVault Design & Use Case

## Overview

The `FeeVault` is a specialized smart contract designed to accumulate native tokens (gas tokens) and automatically split them between bridging to a specific destination chain (e.g., Celestia) and sending to a secondary recipient.

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

## Embedding FeeVault in Genesis

Embedding FeeVault in genesis means pre-deploying the runtime bytecode and setting storage slots directly. The constructor does **not** run, so every needed value must be written into `alloc.storage`.

### 1. Choose the FeeVault address

If you want a deterministic address across chains, compute the CREATE2 address and use that address in `alloc`:

```bash
export OWNER=0xYourOwnerOrAdminProxy
export SALT=0x0000000000000000000000000000000000000000000000000000000000000001
export DEPLOYER=0xYourDeployerAddress
export DESTINATION_DOMAIN=1234
export RECIPIENT_ADDRESS=0x0000000000000000000000000000000000000000000000000000000000000000
export MINIMUM_AMOUNT=0
export CALL_FEE=0
export BRIDGE_SHARE_BPS=10000
export OTHER_RECIPIENT=0x0000000000000000000000000000000000000000

forge script script/DeployFeeVault.s.sol:ComputeFeeVaultAddress
```

If you do not care about CREATE2 determinism, pick any address and use it in `alloc`.

### 2. Get the runtime bytecode

Use the deployed (runtime) bytecode in genesis:

```bash
forge inspect FeeVault deployedBytecode
```

You can also generate the alloc snippet (including code + storage) with the helper script:

```bash
# Required
export OWNER=0xYourOwnerOrAdminProxy

# Optional but recommended for a deterministic address
export DEPLOYER=0xYourDeployerAddress
export SALT=0x0000000000000000000000000000000000000000000000000000000000000001

# If you are not using CREATE2, set the address explicitly
export FEE_VAULT_ADDRESS=0xYourFeeVaultAddress

# Optional configuration (defaults to zero)
export DESTINATION_DOMAIN=1234
export RECIPIENT_ADDRESS=0x0000000000000000000000000000000000000000000000000000000000000000
export MINIMUM_AMOUNT=0
export CALL_FEE=0
export BRIDGE_SHARE_BPS=10000
export OTHER_RECIPIENT=0x0000000000000000000000000000000000000000
export HYP_NATIVE_MINTER=0x0000000000000000000000000000000000000000

forge script script/GenerateFeeVaultAlloc.s.sol -vvv
```

### 3. Set storage slots in alloc

Storage layout is derived from declaration order in `FeeVault.sol`:

| Slot | Variable | Encoding |
|------|----------|----------|
| `0x0` | `hypNativeMinter` | Address (20 bytes, left-padded) |
| `0x1` | `owner` + `destinationDomain` | `0x0000000000000000<destinationDomain 8 hex><owner 40 hex>` |
| `0x2` | `recipientAddress` | bytes32 |
| `0x3` | `minimumAmount` | uint256 |
| `0x4` | `callFee` | uint256 |
| `0x5` | `otherRecipient` | Address (20 bytes, left-padded) |
| `0x6` | `bridgeShareBps` | uint256 |

Notes:

- `owner` must be non-zero, otherwise no one can administer the vault.
- The constructor default (`bridgeShareBps = 10000 when 0`) does **not** apply at genesis. Set `0x2710` (10000) explicitly if you want 100% bridging. The helper script applies this default for you when `BRIDGE_SHARE_BPS=0`.
- `hypNativeMinter` can be zero at genesis, but it must be set before calling `sendToCelestia()`.

Example alloc entry (address key without `0x`):

```json
{
  "alloc": {
    "<FEE_VAULT_ADDRESS_NO_0X>": {
      "balance": "0x0",
      "code": "0x<DEPLOYED_FEE_VAULT_BYTECODE>",
      "storage": {
        "0x0": "0x0000000000000000000000001111111111111111111111111111111111111111",
        "0x1": "0x0000000000000000000004d22222222222222222222222222222222222222222",
        "0x2": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "0x3": "0x0",
        "0x4": "0x0",
        "0x5": "0x0000000000000000000000000000000000000000",
        "0x6": "0x2710"
      }
    }
  }
}
```

### 4. Verify after genesis

Once the node is running with your genesis file, verify the configuration on-chain:

```bash
# Check runtime code exists
cast code <FEE_VAULT_ADDRESS> --rpc-url <YOUR_RPC>

# Inspect full config in one call
cast call <FEE_VAULT_ADDRESS> \
  "getConfig()(address,uint32,bytes32,uint256,uint256,uint256,address,address)" \
  --rpc-url <YOUR_RPC>

# Or read individual storage slots (optional)
cast storage <FEE_VAULT_ADDRESS> 0x0 --rpc-url <YOUR_RPC>
cast storage <FEE_VAULT_ADDRESS> 0x1 --rpc-url <YOUR_RPC>
```

### 5. Wire base fee redirect (optional)

To route base fees into FeeVault from genesis, set `ev_reth.baseFeeSink` to the FeeVault address and `baseFeeRedirectActivationHeight` to `0` in your chainspec (see `README.md` for the full chainspec example).
