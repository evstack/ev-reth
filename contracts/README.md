# EV-Reth Contracts

Smart contracts for EV-Reth, including the FeeVault for collecting and distributing fees.

## AdminProxy

The AdminProxy contract solves the bootstrap problem for admin addresses at genesis. It acts as an intermediary owner/admin for other contracts and precompiles (like the Mint Precompile) when the final admin (e.g., a multisig) is not known at genesis time.

See [AdminProxy documentation](../docs/contracts/admin_proxy.md) for detailed setup and usage instructions.

## FeeVault

The FeeVault contract collects base fees and distributes them between a bridge recipient and an optional secondary recipient. It supports:

- Configurable fee splitting between bridge and another recipient
- Minimum amount thresholds before distributing
- Call fee for incentivizing distribution calls
- Owner-controlled configuration

## Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation)

## Build

```shell
forge build
```

## Test

```shell
forge test
```

## Deploying FeeVault

The FeeVault uses CREATE2 for deterministic addresses across chains.

### Environment Variables

All configuration is set via constructor arguments at deploy time:

| Variable | Required | Description |
|----------|----------|-------------|
| `OWNER` | Yes | Owner address (can configure the vault post-deployment) |
| `SALT` | No | CREATE2 salt (default: `0x0`). Use any bytes32 value |
| `MINIMUM_AMOUNT` | No | Minimum wei to distribute (default: 0) |
| `CALL_FEE` | No | Fee in wei for calling `distribute()` (default: 0) |
| `BRIDGE_SHARE_BPS` | No | Basis points to bridge (default: 10000 = 100%) |
| `OTHER_RECIPIENT` | No* | Address to receive non-bridged portion |

*Required if `BRIDGE_SHARE_BPS` < 10000

**Note:** `BRIDGE_RECIPIENT` must be set via `setBridgeRecipient()` after deployment for the vault to be operational.

### Choosing a Salt

Any bytes32 value works as a salt. Common approaches:

```shell
# Simple approach - just use a version number
export SALT=0x0000000000000000000000000000000000000000000000000000000000000001

# Or hash a meaningful string
export SALT=$(cast keccak "FeeVault-v1")
```

### Compute Address Before Deploying

To see what address will be deployed to without actually deploying:

```shell
export OWNER=0xYourOwnerAddress
export SALT=0x0000000000000000000000000000000000000000000000000000000000000001
export DEPLOYER=0xYourDeployerAddress  # The address that will run the script

forge script script/DeployFeeVault.s.sol:ComputeFeeVaultAddress
```

### Deploy

```shell
# Required
export OWNER=0xYourOwnerAddress
export SALT=0x0000000000000000000000000000000000000000000000000000000000000001

# Optional - configure at deploy time (can also be set later)
export MINIMUM_AMOUNT=1000000000000000000  # 1 ETH in wei
export CALL_FEE=100000000000000  # 0.0001 ETH
export BRIDGE_SHARE_BPS=8000  # 80% to bridge
export OTHER_RECIPIENT=0xOtherAddress

# Dry run (no broadcast)
forge script script/DeployFeeVault.s.sol:DeployFeeVault \
    --rpc-url <RPC_URL>

# Deploy for real
forge script script/DeployFeeVault.s.sol:DeployFeeVault \
    --rpc-url <RPC_URL> \
    --private-key <PRIVATE_KEY> \
    --broadcast
```

### Post-Deployment: Set Bridge Recipient

After deployment, set the bridge recipient address:

```shell
cast send <FEEVAULT_ADDRESS> "setBridgeRecipient(address)" <BRIDGE_RECIPIENT_ADDRESS> \
    --rpc-url <RPC_URL> \
    --private-key <PRIVATE_KEY>
```

## Admin Functions

All functions are owner-only:

| Function | Description |
|----------|-------------|
| `setBridgeRecipient(address)` | Set the bridge recipient address |
| `setMinimumAmount(uint256)` | Set minimum amount to distribute |
| `setCallFee(uint256)` | Set fee for calling distribute |
| `setBridgeShare(uint256)` | Set bridge percentage (basis points) |
| `setOtherRecipient(address)` | Set recipient for non-bridged funds |
| `transferOwnership(address)` | Transfer contract ownership |
