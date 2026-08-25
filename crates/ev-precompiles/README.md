# ev-precompiles

Custom EVM precompiles for Evolve, providing native token supply management, proposer control, and
state-backed deployment permissions.

## Overview

This crate implements custom precompiled contracts that extend the EVM with Evolve-specific functionality. It provides a mint/burn precompile for controlled native token supply management and a proposer-control precompile for execution-owned ev-node proposer rotation.

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
    function mint(address to, uint256 amount) external;
    function burn(address from, uint256 amount) external;
    function addToAllowList(address account) external;
    function removeFromAllowList(address account) external;
    function allowlist(address account) external view returns (bool);
}
```

### Authorization

Only authorized addresses can call state-mutating functionality. Authorization is composed of:

- The **mint admin** address, configured in the chain specification.
- Addresses that the mint admin adds to the precompile's **allowlist** at runtime.

The admin manages the allowlist through the dedicated functions on the precompile interface and can add or remove entries without redeploying contracts.

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

See the [AdminProxy documentation](../../docs/contracts/admin_proxy.md) for a ready-to-use proxy contract that can be deployed at genesis and later upgraded to a multisig.

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
      "mintAdmin": "0x1234567890123456789012345678901234567890",
      "mintPrecompileActivationHeight": 0
    }
  }
}
```

If no mint admin is specified, the precompile is still available but will reject all calls.
Set `mintPrecompileActivationHeight` to the block where the precompile should become callable. For
new networks keep it at `0` so the admin is active from genesis; existing chains can use a higher
value to stage upgrades safely.

### Allowlist Management

The mint admin can delegate minting and burning capabilities to additional addresses by adding them to the allowlist:

```solidity
INativeToken(MINT_PRECOMPILE_ADDR).addToAllowList(operator);
INativeToken(MINT_PRECOMPILE_ADDR).removeFromAllowList(operator);
```

Allowlisted addresses can invoke `mint` and `burn`, but they cannot modify the allowlist itself. Removing an address from the allowlist immediately revokes its permissions.

#### Example Transactions

The allowlist is managed through standard transactions targeting the precompile address. For example, using Foundry's `cast` CLI:

```bash
# Grant operator access (run as the configured mint admin)
cast send --rpc-url $RPC_URL --private-key $ADMIN_KEY \
  0x000000000000000000000000000000000000f100 \
  "addToAllowList(address)" 0xOPERATOR_ADDRESS

# Revoke access later
cast send --rpc-url $RPC_URL --private-key $ADMIN_KEY \
  0x000000000000000000000000000000000000f100 \
  "removeFromAllowList(address)" 0xOPERATOR_ADDRESS
```

Any address added to the allowlist can then call the precompile directly:

```bash
# Allowlisted operator mints 1 ether to a recipient
cast send --rpc-url $RPC_URL --private-key $OPERATOR_KEY \
  0x000000000000000000000000000000000000f100 \
  "mint(address,uint256)" 0xRECIPIENT 1000000000000000000
```

## Proposer Control Precompile

The proposer control precompile stores the ev-node proposer that should sign the next block. It is
used by the ev-node EVM execution adapter to populate ADR-023's `NextProposerAddress` from execution
state.

### Address

```text
0x000000000000000000000000000000000000f101
```

### Interface

```solidity
interface IProposerControl {
    function nextProposer() external view returns (bytes32);
    function setNextProposer(bytes32 proposer) external;
    function admin() external view returns (address);
}
```

The proposer is an opaque 32-byte value rather than an `address` so it can hold proposer
identities that are not 20-byte EVM addresses (ev-node signer keys). For an EVM address,
left-pad it to 32 bytes. The precompile only rejects the zero value; it does not validate
that the bytes encode a usable proposer identity, so the admin must submit a correct value.

### Configuration

```json
{
  "config": {
    "evolve": {
      "proposerControlAdmin": "0x1234567890123456789012345678901234567890",
      "proposerControlActivationHeight": 0,
      "initialNextProposer": "0x000000000000000000000000abcdefabcdefabcdefabcdefabcdefabcdefabcd"
    }
  }
}
```

`initialNextProposer` is a 32-byte value (a 20-byte address must be left-padded, as above).
For existing chains, set `proposerControlActivationHeight` to a future block and upgrade all nodes
before that height. `initialNextProposer` should be the currently active proposer so reads are stable
before the first rotation transaction.

`proposerControlAdmin` is fixed in the chainspec. Production networks should set it to
[AdminProxy](../../docs/contracts/admin_proxy.md) (or another governance contract) so the
controlling key can be rotated later. An EOA admin cannot be changed without a hard fork.
Existing-chain upgrades are in [ADR 0004](../../docs/adr/ADR-0004-proposer-rotation-precompile.md) and the [v0.5.0 upgrade guide](../../docs/UPGRADE-v0.5.0.md).

### Operations

If the admin is an EOA (development only):

```bash
cast send --rpc-url $RPC_URL --private-key $ADMIN_KEY \
  0x000000000000000000000000000000000000f101 \
  "setNextProposer(bytes32)" \
  0x000000000000000000000000$(echo $NEXT_PROPOSER_ADDR | cut -c3-)
```

If the admin is AdminProxy (production), the proxy owner calls `execute`:

```bash
cast send --rpc-url $RPC_URL --private-key $OWNER_KEY \
  0x000000000000000000000000000000000000Ad00 \
  "execute(address,bytes)" \
  0x000000000000000000000000000000000000f101 \
  $(cast calldata "setNextProposer(bytes32)" \
    0x000000000000000000000000$(echo $NEXT_PROPOSER_ADDR | cut -c3-))
```

Before activation height, a call to `0xF101` is a normal empty-account call: it succeeds and
writes nothing. Confirm the height has passed before treating a successful receipt as a rotation.

The stored proposer can be read through either the precompile ABI or ev-reth's convenience RPC
(only registered when `proposerControlAdmin` is configured):

```bash
cast call --rpc-url $RPC_URL \
  0x000000000000000000000000000000000000f101 \
  "nextProposer()(bytes32)"

cast rpc --rpc-url $RPC_URL evolve_getNextProposer latest
```

### Errors

- `unauthorized caller`: `msg.sender` is not `proposerControlAdmin`
- `proposer cannot be zero`: rejected so a rotation cannot clear the stored value
- `state change during static call`: `setNextProposer` during `STATICCALL` / `eth_call`

Invalid ABI data also halts the precompile. None of these emit logs.

### Safety

- Authorization is `msg.sender == proposerControlAdmin`. Contracts such as AdminProxy work because
  they `CALL` the precompile; `DELEGATECALL` would present the original EOA and be rejected.
- The admin can set any non-zero `bytes32`. A garbage value will stall ev-node until another
  rotation. Confirm the new sequencer is online with that signer before the next block.
- Compromise of the admin (or AdminProxy owner) is compromise of sequencer selection.
- `evolve_getNextProposer` is a public read of execution state. It is not registered when the
  precompile is disabled, so ev-node treats method-not-found as "feature off".

## Deployment Permissions Precompile

The optional deployment-permissions precompile is installed at
`0x000000000000000000000000000000000000f102` when `deployAllowlistAdmin` is configured and
`deployAllowlistPrecompileActivationHeight` is reached.

```solidity
interface IDeployPermissions {
    function addDeployer(address account) external;
    function removeDeployer(address account) external;
    function setEnabled(bool enabled) external;
    function isDeployerAllowed(address account) external view returns (bool);
    function isEnabled() external view returns (bool);
    function deployerCount() external view returns (uint256);
    function admin() external view returns (address);
}
```

The genesis `deployAllowlist` is the baseline. Enforcement is enabled when its state flag is unset,
so no bootstrap call is required. `setEnabled(false)` allows all top-level deployments without
discarding membership. Member changes are permitted while disabled and apply when enforcement is
re-enabled. The active set is capped at 1024 and excludes the zero address.

The admin is fixed by chainspec and should normally be an AdminProxy, multisig, or governance
contract. Read the interface through standard `eth_call`; no custom RPC is required. See the
[permissioned EVM guide](../../docs/guide/permissioned-evm.md) for rollout and security details.
