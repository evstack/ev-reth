# ev-precompiles

Custom EVM precompiles for Evolve, providing native token supply management functionality.

## Overview

This crate implements custom precompiled contracts that extend the EVM with Evolve-specific functionality. Currently, it provides a mint/burn precompile that allows controlled manipulation of native token supply.

## Mint Precompile

The mint precompile enables authorized minting and burning of the native token (ETH equivalent) in the Evolve execution environment.

### Address

```
0x000000000000000000000000000000000000f100
```

The precompile is deployed at a reserved address in the precompile address space.

### Interface

```solidity
interface INativeToken {
    function mint(address to, uint256 amount);
    function burn(address from, uint256 amount);
}
```

### Authorization

Only the **mint admin** address can call this precompile. The mint admin is configured in the chain specification:

```json
{
  "config": {
    "evolve": {
      "mintAdmin": "0x..."
    }
  }
}
```

Calls from any other address will be rejected with an "unauthorized caller" error.

### Operations

#### Mint

Mints new native tokens to a specified address.

**Parameters:**
- `to` (address): Recipient address
- `amount` (uint256): Amount to mint in wei

**Behavior:**
1. Verifies caller is the authorized mint admin
2. Creates the recipient account if it doesn't exist
3. Increases the recipient's balance by the specified amount
4. Marks the account as touched (for EVM state change tracking)

**Gas:** Returns unused gas (precompile consumes minimal gas)

**Errors:**
- `unauthorized caller`: Caller is not the mint admin
- `balance overflow`: Adding the amount would overflow uint256

#### Burn

Burns native tokens from a specified address.

**Parameters:**
- `from` (address): Address to burn tokens from
- `amount` (uint256): Amount to burn in wei

**Behavior:**
1. Verifies caller is the authorized mint admin
2. Ensures the target account exists
3. Decreases the target's balance by the specified amount
4. Marks the account as touched

**Gas:** Returns unused gas (precompile consumes minimal gas)

**Errors:**
- `unauthorized caller`: Caller is not the mint admin
- `insufficient balance`: Account doesn't have enough balance to burn

### Usage Pattern

The typical usage pattern involves deploying a proxy contract at the mint admin address that delegates calls to this precompile.

This pattern allows the mint admin to be a smart contract with custom authorization logic (multisig, governance, etc.) rather than a simple EOA.

## Implementation Details

### Account Creation

The precompile automatically creates accounts that don't exist when minting to them. This ensures that:
- Tokens can be minted to any address, including those not yet active on-chain
- The account is properly marked as created in the EVM state
- The account is touched for accurate state tracking

### Balance Manipulation

The precompile directly modifies account balances in the EVM state using the `EvmInternals` API. This provides:
- **Direct state access**: No need for complex transfer mechanisms
- **Overflow protection**: All arithmetic is checked
- **State consistency**: Accounts are properly touched for journaling

### Safety Guarantees

1. **Authorization**: Only the designated mint admin can mint/burn
2. **Arithmetic Safety**: All balance operations are checked for overflow/underflow
3. **State Consistency**: Accounts are properly created and touched
4. **Gas Handling**: Unused gas is returned to the caller

## Configuration

The mint admin is configured in the chain specification. See `crates/node/src/config.rs` for configuration parsing.

### Chain Spec Example

```json
{
  "config": {
    "chainId": 1234,
    "evolve": {
      "mintAdmin": "0x1234567890123456789012345678901234567890"
    }
  }
}
```

If no mint admin is specified, the precompile is still available but will reject all calls.
