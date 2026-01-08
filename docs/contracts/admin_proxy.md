# AdminProxy Design & Use Case

## Overview

The `AdminProxy` is a smart contract that solves the bootstrap problem for admin addresses at genesis. It acts as an intermediary owner/admin for other contracts and precompiles when the final admin (e.g., a multisig) is not known at genesis time.

## Problem Statement

Several components in ev-reth require admin addresses configured at genesis:

1. **Mint Precompile**: Requires `mintAdmin` in chainspec to manage the allowlist
2. **FeeVault**: Requires an `owner` address in its constructor

The challenge: these admin addresses often need to be multisigs (like Safe) for security, but multisigs cannot be deployed at genesis because they require transactions to be created.

## Solution

Deploy `AdminProxy` at genesis with `owner = address(0)`. Post-genesis:

1. An EOA claims ownership
2. The multisig is deployed
3. Ownership is transferred to the multisig via two-step transfer

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

### Step 2: Get the Runtime Bytecode

There are two ways to get the bytecode:

**Option A: Use the helper script**

```bash
forge script script/GenerateAdminProxyAlloc.s.sol -vvv
```

This outputs the complete alloc entry you can copy into your genesis file.

**Option B: Get bytecode directly from artifacts**

After building, the runtime bytecode is in the compiled artifacts:

```bash
# Extract just the deployed bytecode (not creation code)
cat out/AdminProxy.sol/AdminProxy.json | jq -r '.deployedBytecode.object'
```

This outputs the hex string starting with `0x608060...`.

### Step 3: Create the Genesis Alloc Entry

The genesis `alloc` section pre-deploys contracts at specific addresses. For AdminProxy:

```json
{
  "alloc": {
    "000000000000000000000000000000000000Ad00": {
      "balance": "0x0",
      "code": "0x608060405234801561001057600080fd5b50600436106100935760003560e01c80638da5cb5b116100665780638da5cb5b146100fa578063b0e21e8a1461010b578063e30c39781461011e578063f2fde38b1461012f578063fe0d94c11461014257600080fd5b80631a69523014610098578063715018a6146100ad57806374e6310e146100b557806379ba5097146100f2575b600080fd5b6100ab6100a63660046108d5565b610155565b005b6100ab6101d5565b6100c86100c33660046108f7565b6101e9565b604080516001600160a01b0390931683526020830191909152015b60405180910390f35b6100ab610283565b6000546001600160a01b03166100c8565b6100ab610119366004610938565b6102f8565b6001546001600160a01b03166100c8565b6100ab61013d3660046108d5565b610410565b6100ab6101503660046109ab565b610481565b61015d610528565b6001600160a01b03811661018457604051631e4fbdf760e01b815260040160405180910390fd5b600180546001600160a01b0319166001600160a01b0383169081179091556000546040516001600160a01b03909116907f38d16b8cac22d99fc7c124b9cd0de2d3fa1faef420bfe791d8c362d765e2270090600090a350565b6101dd610528565b6101e76000610555565b565b60008061021d846040518060400160405280600e81526020016d2737ba1030b63637bbb2b2103a3960911b8152506105a5565b9050600080856001600160a01b031683866040516102449291906001600160a01b03929092168252602082015260400190565b6000604051808303816000875af1925050503d8060008114610282576040519150601f19603f3d011682016040523d82523d6000602084013e610287565b606091505b5091509150816102ac576102ae604051806060016040528060228152602001610b2f60229139836105d1565b505b6040805180820182526001600160a01b038089168252602080830188905283518581529182018690528451928301939093529051909116907f6e9b6e3f1f8e21e9d5e8f5e8e5e8e5e8e5e8e5e8e5e8e5e8e5e8e5e8e5e8e5e89181900360600190a25090925050509250929050565b6001546001600160a01b031633146102e85760405163118cdaa760e01b81523360048201526024015b60405180910390fd5b6001805460006001600160a01b0319918216178255805482166001600160a01b03831690811782556040519192909116907f8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0908490a350565b610300610528565b82811461032057604051634ec4810560e11b815260040160405180910390fd5b60005b838110156104095760008086868481811061034057610340610a4d565b90506020020160208101906103559190610938565b6001600160a01b031685858581811061037057610370610a4d565b905060200281019061038291906109f4565b604051610390929190610a3d565b6000604051808303816000865af19150503d80600081146103cd576040519150601f19603f3d011682016040523d82523d6000602084013e6103d2565b606091505b5091509150816103ff576103f9604051806060016040528060228152602001610b2f60229139836105d1565b50610400565b5b50600101610323565b5050505050565b610418610528565b6001600160a01b03811661043f57604051631e4fbdf760e01b815260040160405180910390fd5b600180546001600160a01b0319166001600160a01b0383169081179091556000546040516001600160a01b03909116907f38d16b8cac22d99fc7c124b9cd0de2d3fa1faef420bfe791d8c362d765e2270090600090a350565b610489610528565b6000826001600160a01b031682846040516104a49190610a63565b60006040518083038185875af1925050503d80600081146104e1576040519150601f19603f3d011682016040523d82523d6000602084013e6104e6565b606091505b505090508061052257610522604051806060016040528060228152602001610b2f6022913960405180602001604052806000815250905090506105d1565b50505050565b6000546001600160a01b031633146101e75760405163118cdaa760e01b81523360048201526024016102df565b600080546001600160a01b038381166001600160a01b0319831681178455604051919092169283917f8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e09190a35050565b6060828260405160200161057b92919091825260601b6bffffffffffffffffffffffff1916602082015260340190565b604051602081830303815290604052905092915050565b60606105e183836040518060400160405280601e81526020017f416464726573733a206c6f772d6c6576656c2063616c6c206661696c65640000815250610648565b9392505050565b81516000036105f8575050565b8060000361060557505050565b815160208301fd5b634e487b7160e01b600052604160045260246000fd5b604051601f8201601f1916810167ffffffffffffffff811182821017156106485761064861060d565b604052919050565b919050565b600067ffffffffffffffff8211156106775761067761060d565b50601f01601f191660200190565b600082601f83011261069657600080fd5b81356106a96106a48261065d565b610623565b8181528460208386010111156106be57600080fd5b816020850160208301376000918101602001919091529392505050565b6000806000606084860312156106f057600080fd5b83356001600160a01b038116811461070757600080fd5b925060208401359150604084013567ffffffffffffffff81111561072a57600080fd5b61073686828701610685565b9150509250925092565b60008083601f84011261075257600080fd5b50813567ffffffffffffffff81111561076a57600080fd5b6020830191508360208260051b850101111561078557600080fd5b9250929050565b600080600080604085870312156107a257600080fd5b843567ffffffffffffffff808211156107ba57600080fd5b6107c688838901610740565b909650945060208701359150808211156107df57600080fd5b506107ec87828801610740565b95989497509550505050565b60005b838110156108135781810151838201526020016107fb565b50506000910152565b600081518084526108348160208601602086016107f8565b601f01601f19169290920160200192915050565b6001600160a01b038716815260208101869052604081018590526060810184905260c06080820181905260009061088190830185610823565b82810360a08401526108938185610823565b9998505050505050505050565b6000602082840312156108b257600080fd5b5035919050565b6001600160a01b03811681146108ce57600080fd5b50565b6000602082840312156108e357600080fd5b81356108ee816108b9565b9392505050565b6000806040838503121561090857600080fd5b8235610913816108b9565b9150602083013567ffffffffffffffff81111561092f57600080fd5b61093b85828601610685565b9150509250929050565b6000806000806040858703121561095b57600080fd5b843567ffffffffffffffff8082111561097357600080fd5b61097f88838901610740565b9650602087013591508082111561099557600080fd5b506107ec87828801610740565b634e487b7160e01b600052603260045260246000fd5b6000602082840312156109c957600080fd5b81356001600160a01b03811681146108ee57600080fd5b8183823760009101908152919050565b600082516109ff8184602087016107f8565b9190910192915050565b60008151808452610a218160208601602086016107f8565b601f01601f19169290920160200192915050565b6020815260006105e16020830184610a0956fe416464726573733a2063616c6c206661696c656420776974686f757420726576657274696e67a2646970667358221220...",
      "storage": {}
    }
  }
}
```

**Important notes about the alloc entry:**

1. **Address format**: The address key does NOT have the `0x` prefix in the alloc section
2. **Code format**: The code value MUST have the `0x` prefix
3. **Storage**: Empty `{}` because AdminProxy initializes `owner = address(0)` and `pendingOwner = address(0)`, which are the default zero values (no explicit storage needed)

### Step 4: Complete Genesis File Example

Here's a complete example showing how AdminProxy fits into the full genesis file:

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
      "storage": {}
    }
  }
}
```

### Step 5: Verify the Setup

After creating your genesis file, you can verify the AdminProxy is correctly configured:

1. **Start the node** with your genesis file
2. **Query the contract code** at the proxy address to confirm deployment:

   ```bash
   cast code 0x000000000000000000000000000000000000Ad00 --rpc-url <YOUR_RPC>
   ```

3. **Check owner is zero** (ready for claiming):

   ```bash
   cast call 0x000000000000000000000000000000000000Ad00 "owner()" --rpc-url <YOUR_RPC>
   # Should return 0x0000000000000000000000000000000000000000
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

### 1. Claim Ownership

An authorized EOA claims initial ownership:

```solidity
AdminProxy proxy = AdminProxy(0x000000000000000000000000000000000000Ad00);
proxy.claimOwnership(); // First caller becomes owner
```

### 2. Deploy Multisig

Deploy your multisig (e.g., Safe) through normal transaction flow.

### 3. Transfer Ownership

Two-step transfer to multisig for safety:

```solidity
// Step 1: Current owner initiates transfer
proxy.transferOwnership(multisigAddress);

// Step 2: Multisig accepts (requires multisig transaction)
proxy.acceptOwnership(); // Called by multisig
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

### Zero Owner Bootstrap

The contract initializes with `owner = address(0)`. This allows `claimOwnership()` to be called by the first authorized party post-genesis. Once claimed, this path is closed.

### Call Forwarding

The `execute` function forwards calls with the proxy as `msg.sender`. Target contracts see the proxy as the caller, not the original sender. This is intentional for the admin pattern.

## Contract Interface

| Function | Description | Access |
|----------|-------------|--------|
| `claimOwnership()` | Claim ownership when owner is zero | Anyone (once) |
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
