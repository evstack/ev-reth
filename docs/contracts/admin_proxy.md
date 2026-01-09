# AdminProxy Design & Use Case

## Overview

The `AdminProxy` is a smart contract that solves the bootstrap problem for admin addresses at genesis. It acts as an intermediary owner/admin for other contracts and precompiles when the final admin (e.g., a multisig) is not known at genesis time.

## Problem Statement

Several components in ev-reth require admin addresses configured at genesis:

1. **Mint Precompile**: Requires `mintAdmin` in chainspec to manage the allowlist
2. **FeeVault**: Requires an `owner` address in its constructor

The challenge: these admin addresses often need to be multisigs (like Safe) for security, but multisigs cannot be deployed at genesis because they require transactions to be created.

## Solution

Deploy `AdminProxy` at genesis with `owner` set directly in storage slot 0. This eliminates any race condition and ensures the designated admin has control from block 0.

Post-genesis:

1. The owner (set at genesis) can immediately use the proxy
2. When ready, deploy the multisig
3. Transfer ownership to multisig via two-step transfer (`transferOwnership` + `acceptOwnership`)

The proxy then forwards admin calls to the underlying contracts/precompiles.

## Architecture

```
                    ┌─────────────────┐
                    │    Multisig     │
                    │   (Safe, etc)   │
                    └────────┬────────┘
                             │
                             │ owns
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                      AdminProxy                              │
│  - owner: address (initially 0, then EOA, then multisig)    │
│  - execute(target, data): forward calls                      │
│  - executeBatch(targets, datas): batch operations           │
└──────────────┬────────────────────────┬─────────────────────┘
               │                        │
               │ admin calls            │ owner calls
               ▼                        ▼
    ┌──────────────────┐      ┌──────────────────┐
    │  Mint Precompile │      │    FeeVault      │
    │    (0xF100)      │      │                  │
    └──────────────────┘      └──────────────────┘
```

## Genesis Configuration

This section provides detailed instructions for deploying AdminProxy at genesis.

### Step 1: Build the Contract

```bash
cd contracts
forge build
```

### Step 2: Generate the Genesis Alloc Entry

**Option A: Use the helper script (recommended)**

Set the `OWNER` environment variable to your initial admin EOA address:

```bash
OWNER=0xYourEOAAddress forge script script/GenerateAdminProxyAlloc.s.sol -vvv
```

This outputs the complete alloc entry with bytecode and storage, ready to copy into your genesis file.

**Option B: Get bytecode directly from artifacts**

After building, the runtime bytecode is in the compiled artifacts:

```bash
# Extract just the deployed bytecode (not creation code)
cat out/AdminProxy.sol/AdminProxy.json | jq -r '.deployedBytecode.object'
```

This outputs the hex string starting with `0x608060...`. You'll need to manually construct the storage entry for the owner (see Step 3).

### Step 3: Create the Genesis Alloc Entry

The genesis `alloc` section pre-deploys contracts at specific addresses. For AdminProxy, you must set the owner in storage slot 0.

**Storage Layout:**

| Slot | Variable | Type |
|------|----------|------|
| 0 | `owner` | `address` |
| 1 | `pendingOwner` | `address` |

**Converting owner address to storage value:**

The owner address must be left-padded to 32 bytes. For example, if your owner EOA is `0x1234567890abcdef1234567890abcdef12345678`:

```
Storage slot 0x0 = 0x0000000000000000000000001234567890abcdef1234567890abcdef12345678
```

**Example alloc entry:**

```json
{
  "alloc": {
    "000000000000000000000000000000000000Ad00": {
      "balance": "0x0",
      "code": "0x<YOUR_BYTECODE_HERE>",
      "storage": {
        "0x0": "0x0000000000000000000000001234567890abcdef1234567890abcdef12345678"
      }
    }
  }
}
```

**Important notes:**

1. **Address format**: The address key does NOT have the `0x` prefix in the alloc section
2. **Code format**: The code value MUST have the `0x` prefix
3. **Storage key**: Must be `"0x0"` (slot 0 for owner)
4. **Storage value**: Owner address left-padded to 32 bytes with `0x` prefix

### Step 4: Complete Genesis File Example

Here's a complete example showing how AdminProxy fits into the full genesis file.

In this example, the owner EOA is `0xYourEOAAddressHere` (replace with your actual address):

```json
{
  "config": {
    "chainId": 1,
    "homesteadBlock": 0,
    "eip150Block": 0,
    "eip155Block": 0,
    "eip158Block": 0,
    "byzantiumBlock": 0,
    "constantinopleBlock": 0,
    "petersburgBlock": 0,
    "istanbulBlock": 0,
    "berlinBlock": 0,
    "londonBlock": 0,
    "parisBlock": 0,
    "shanghaiTime": 0,
    "cancunTime": 0,
    "terminalTotalDifficulty": 0,
    "terminalTotalDifficultyPassed": true,
    "evolve": {
      "baseFeeSink": "0x00000000000000000000000000000000000000fe",
      "baseFeeRedirectActivationHeight": 0,
      "mintAdmin": "0x000000000000000000000000000000000000Ad00",
      "mintPrecompileActivationHeight": 0,
      "contractSizeLimit": 131072,
      "contractSizeLimitActivationHeight": 0
    }
  },
  "difficulty": "0x1",
  "gasLimit": "0x1c9c380",
  "alloc": {
    "000000000000000000000000000000000000Ad00": {
      "balance": "0x0",
      "code": "0x<YOUR_BYTECODE_HERE>",
      "storage": {
        "0x0": "0x000000000000000000000000<YOUR_EOA_ADDRESS_WITHOUT_0x_PREFIX>"
      }
    },
    "<YOUR_EOA_ADDRESS_WITHOUT_0x_PREFIX>": {
      "balance": "0x56bc75e2d63100000"
    }
  }
}
```

**Note:** The owner EOA must also be funded with gas at genesis to execute transactions. In the example above, `0x56bc75e2d63100000` equals 100 ETH in wei.

**Example with concrete addresses:**

If your owner EOA is `0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266`:

```json
"storage": {
  "0x0": "0x000000000000000000000000f39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
}
```

### Step 5: Verify the Setup

After creating your genesis file, you can verify the AdminProxy is correctly configured:

1. **Start the node** with your genesis file
2. **Query the contract code** at the proxy address to confirm deployment:

   ```bash
   cast code 0x000000000000000000000000000000000000Ad00 --rpc-url <YOUR_RPC>
   ```

3. **Verify owner is set correctly**:

   ```bash
   cast call 0x000000000000000000000000000000000000Ad00 "owner()" --rpc-url <YOUR_RPC>
   # Should return your EOA address (e.g., 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266)
   ```

### Step 6: Deploy FeeVault with Proxy as Owner

When deploying FeeVault (post-genesis), use the AdminProxy address as the owner:

```bash
OWNER=0x000000000000000000000000000000000000Ad00 \
forge script script/DeployFeeVault.s.sol --broadcast --rpc-url <YOUR_RPC>
```

Alternatively, if deploying FeeVault at genesis too, add it to the alloc section with its storage slot 0 (owner) set to the proxy address:

```json
{
  "alloc": {
    "000000000000000000000000000000000000Ad00": {
      "balance": "0x0",
      "code": "0x<ADMIN_PROXY_BYTECODE>",
      "storage": {}
    },
    "<FEE_VAULT_ADDRESS>": {
      "balance": "0x0",
      "code": "0x<FEE_VAULT_BYTECODE>",
      "storage": {
        "0x0": "0x000000000000000000000000000000000000000000000000000000000000Ad00"
      }
    }
  }
}
```

Note: FeeVault has additional storage slots that need to be set. See `docs/contracts/fee_vault.md` for details.

## Post-Genesis Setup

Since the owner is set at genesis, no claiming is required. The designated EOA can immediately use the proxy.

### 1. Verify Ownership

Confirm the owner was set correctly:

```bash
cast call 0x000000000000000000000000000000000000Ad00 "owner()" --rpc-url <YOUR_RPC>
# Should return your EOA address
```

### 2. Deploy Multisig

Deploy your multisig (e.g., Safe) through normal transaction flow.

### 3. Transfer Ownership

Two-step transfer to multisig for safety:

```solidity
AdminProxy proxy = AdminProxy(0x000000000000000000000000000000000000Ad00);

// Step 1: Current owner initiates transfer
proxy.transferOwnership(multisigAddress);

// Step 2: Multisig accepts (requires multisig transaction)
// This must be called FROM the multisig
proxy.acceptOwnership();
```

Using cast:

```bash
# Step 1: Owner initiates transfer
cast send 0x000000000000000000000000000000000000Ad00 \
  "transferOwnership(address)" <MULTISIG_ADDRESS> \
  --private-key <OWNER_PRIVATE_KEY> \
  --rpc-url <YOUR_RPC>

# Step 2: Multisig accepts (execute via multisig UI/CLI)
# The multisig must call: acceptOwnership()
```

## Usage Examples

### Managing Mint Precompile Allowlist

```solidity
AdminProxy proxy = AdminProxy(ADMIN_PROXY_ADDRESS);

// Add address to allowlist
proxy.execute(
    MINT_PRECOMPILE,
    abi.encodeWithSignature("addToAllowList(address)", userAddress)
);

// Remove from allowlist
proxy.execute(
    MINT_PRECOMPILE,
    abi.encodeWithSignature("removeFromAllowList(address)", userAddress)
);

// Batch add multiple addresses
address[] memory targets = new address[](3);
bytes[] memory datas = new bytes[](3);
targets[0] = targets[1] = targets[2] = MINT_PRECOMPILE;
datas[0] = abi.encodeWithSignature("addToAllowList(address)", user1);
datas[1] = abi.encodeWithSignature("addToAllowList(address)", user2);
datas[2] = abi.encodeWithSignature("addToAllowList(address)", user3);
proxy.executeBatch(targets, datas);
```

### Managing FeeVault

```solidity
AdminProxy proxy = AdminProxy(ADMIN_PROXY_ADDRESS);
FeeVault vault = FeeVault(FEE_VAULT_ADDRESS);

// Update minimum amount
proxy.execute(
    address(vault),
    abi.encodeWithSignature("setMinimumAmount(uint256)", 2 ether)
);

// Update bridge share
proxy.execute(
    address(vault),
    abi.encodeWithSignature("setBridgeShare(uint256)", 8000) // 80%
);
```

## Security Considerations

### Two-Step Ownership Transfer

The proxy uses a two-step transfer pattern (`transferOwnership` + `acceptOwnership`) to prevent accidental transfers to wrong addresses. The pending owner must explicitly accept.

### Cancel Transfer

If a transfer was initiated to the wrong address, the current owner can cancel:

```solidity
proxy.cancelTransfer();
```

### Genesis Storage Initialization

The owner is set directly in storage slot 0 at genesis. This eliminates race conditions and ensures the designated admin has control from block 0. No `claimOwnership()` function exists, so there's no risk of front-running.

### Call Forwarding

The `execute` function forwards calls with the proxy as `msg.sender`. Target contracts see the proxy as the caller, not the original sender. This is intentional for the admin pattern.

## Contract Interface

| Function | Description | Access |
|----------|-------------|--------|
| `owner()` | Current owner address | View |
| `pendingOwner()` | Pending owner for two-step transfer | View |
| `transferOwnership(address)` | Start two-step transfer | Owner |
| `acceptOwnership()` | Complete two-step transfer | Pending owner |
| `cancelTransfer()` | Cancel pending transfer | Owner |
| `execute(address, bytes)` | Forward single call | Owner |
| `executeBatch(address[], bytes[])` | Forward multiple calls | Owner |
| `executeWithValue(address, bytes, uint256)` | Forward call with ETH | Owner |

## Events

| Event | Description |
|-------|-------------|
| `OwnershipTransferStarted(address, address)` | Transfer initiated |
| `OwnershipTransferred(address, address)` | Transfer completed |
| `Executed(address, bytes, bytes)` | Call forwarded |

## Recommended Address

We suggest deploying AdminProxy at `0x000000000000000000000000000000000000Ad00` for easy identification. The `Ad` prefix suggests "Admin".
