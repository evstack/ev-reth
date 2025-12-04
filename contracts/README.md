# EV-Reth Contracts

Smart contracts for EV-Reth, including the FeeVault for bridging collected fees to Celestia.

## FeeVault

The FeeVault contract collects base fees and bridges them to Celestia via Hyperlane. It supports:

- Configurable fee splitting between bridge and another recipient
- Minimum amount thresholds before bridging
- Call fee for incentivizing bridge calls
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
| `DESTINATION_DOMAIN` | Yes* | Hyperlane destination chain ID |
| `RECIPIENT_ADDRESS` | Yes* | Recipient on destination chain (bytes32, left-padded) |
| `MINIMUM_AMOUNT` | No | Minimum wei to bridge (default: 0) |
| `CALL_FEE` | No | Fee in wei for calling `sendToCelestia()` (default: 0) |
| `BRIDGE_SHARE_BPS` | No | Basis points to bridge (default: 10000 = 100%) |
| `OTHER_RECIPIENT` | No** | Address to receive non-bridged portion |

*Required for the vault to be operational (can be set to 0 at deploy and configured later via setters)
**Required if `BRIDGE_SHARE_BPS` < 10000

**Note:** `HYP_NATIVE_MINTER` must be set via `setHypNativeMinter()` after deployment for the vault to be operational.

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
export DESTINATION_DOMAIN=1234
export RECIPIENT_ADDRESS=0x000000000000000000000000...  # bytes32, left-padded cosmos address
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

### Post-Deployment: Set HypNativeMinter

After deploying the HypNativeMinter contract, link it to the FeeVault:

```shell
cast send <FEEVAULT_ADDRESS> "setHypNativeMinter(address)" <HYP_NATIVE_MINTER_ADDRESS> \
    --rpc-url <RPC_URL> \
    --private-key <PRIVATE_KEY>
```

### Converting Cosmos Addresses to bytes32

The `recipientAddress` must be a bytes32. To convert a bech32 Cosmos address:

1. Decode the bech32 to get the 20-byte address
2. Left-pad with zeros to 32 bytes

Example using cast:

```shell
# Left-pad a 20-byte address to 32 bytes
cast pad --left --len 32 1234567890abcdef1234567890abcdef12345678
# Output: 0x0000000000000000000000001234567890abcdef1234567890abcdef12345678
```

Note: When calling `transferRemote()` via cast, you may need to omit the `0x` prefix depending on your invocation method.

## Admin Functions

All functions are owner-only:

| Function | Description |
|----------|-------------|
| `setHypNativeMinter(address)` | Set the Hyperlane minter contract |
| `setRecipient(uint32, bytes32)` | Set destination domain and recipient |
| `setMinimumAmount(uint256)` | Set minimum amount to bridge |
| `setCallFee(uint256)` | Set fee for calling sendToCelestia |
| `setBridgeShare(uint256)` | Set bridge percentage (basis points) |
| `setOtherRecipient(address)` | Set recipient for non-bridged funds |
| `transferOwnership(address)` | Transfer contract ownership |
