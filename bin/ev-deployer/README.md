# EV Deployer

CLI tool for deploying ev-reth contracts. It reads a declarative TOML config and either embeds contracts into a chain's genesis state or deploys them to a live chain via CREATE2.

## Modes of Operation

EV Deployer has two deployment modes:

| Mode | When to use | What it does |
|------|-------------|-------------|
| **genesis** | Before the chain starts | Produces JSON alloc entries to embed contracts into the genesis state. No RPC needed. |
| **deploy** | On a running chain | Deploys contracts via CREATE2 through the deterministic deployer. Requires RPC + signer. |

Both modes read the same TOML config. The `address` field in each contract section is used by `genesis` to place the contract at that exact address. In `deploy` mode, addresses are computed deterministically via CREATE2 and the config `address` is ignored.

## Quick Start

```bash
# 1. Generate a config pre-populated for your chain
ev-deployer init --chain-id 42170 --permit2 --output deploy.toml

# 2a. Genesis mode: embed into genesis state
ev-deployer genesis --config deploy.toml --merge-into genesis.json --output genesis-out.json

# 2b. Deploy mode: deploy to a live chain
ev-deployer deploy \
    --config deploy.toml \
    --rpc-url http://localhost:8545 \
    --private-key 0x... \
    --state deploy-state.json
```

## Building

```bash
just build-deployer
```

The binary is output to `target/release/ev-deployer`.

## Commands

### `init`

Generate a starter config file.

```bash
# Bare template (all contracts commented out)
ev-deployer init

# Pre-populated with chain ID, Permit2, and deterministic deployer
ev-deployer init --chain-id 42170 --permit2 --deterministic-deployer

# Full config with all contracts
ev-deployer init \
    --chain-id 42170 \
    --permit2 \
    --admin-proxy-owner 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 \
    --output deploy.toml
```

| Flag | Description |
|------|-------------|
| `--output <PATH>` | Write to file instead of stdout |
| `--chain-id <ID>` | Set the chain ID (defaults to 0) |
| `--permit2` | Enable Permit2 with its canonical address |
| `--deterministic-deployer` | Enable the deterministic deployer (Nick's factory) with its canonical address |
| `--admin-proxy-owner <ADDR>` | Enable AdminProxy with the given owner |

### `genesis`

Generate genesis alloc JSON from a config.

```bash
# Print alloc to stdout
ev-deployer genesis --config deploy.toml

# Write to file
ev-deployer genesis --config deploy.toml --output alloc.json

# Merge into an existing genesis file
ev-deployer genesis --config deploy.toml --merge-into genesis.json --output genesis-out.json

# Overwrite existing addresses when merging
ev-deployer genesis --config deploy.toml --merge-into genesis.json --output genesis-out.json --force

# Also export an address manifest
ev-deployer genesis --config deploy.toml --addresses-out addresses.json
```

In genesis mode, every configured contract must have an `address` field.

### `deploy`

Deploy contracts to a live chain via CREATE2.

```bash
ev-deployer deploy \
    --config deploy.toml \
    --rpc-url http://localhost:8545 \
    --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
    --state deploy-state.json \
    --addresses-out addresses.json
```

| Flag | Env var | Description |
|------|---------|-------------|
| `--config <PATH>` | | Path to the TOML config |
| `--rpc-url <URL>` | `EV_DEPLOYER_RPC_URL` | RPC endpoint of the target chain |
| `--private-key <HEX>` | `EV_DEPLOYER_PRIVATE_KEY` | Hex-encoded private key for signing |
| `--state <PATH>` | | Path to the state file (created if absent) |
| `--addresses-out <PATH>` | | Write a JSON address manifest |

The deploy pipeline:

1. Connects to the RPC and verifies the chain ID matches the config.
2. Checks that the deterministic deployer (`0x4e59b44847b379578588920ca78fbf26c0b4956c`) exists on-chain.
3. Deploys each configured contract via CREATE2.
4. Verifies that the on-chain bytecode matches the expected bytecode (including patched immutables).

The `address` field in the config is **ignored** in deploy mode — addresses come from the CREATE2 computation.

#### State file and resumability

The `--state` file tracks deployment progress. On first run it generates a random CREATE2 salt and records which contracts have been deployed. If the process is interrupted, re-running with the same state file resumes where it left off.

Immutability rules protect against accidental misconfiguration on resume:

- The `chain_id` cannot change between runs.
- A contract that was configured in the original run cannot be removed.
- New contracts can be added to subsequent runs.

### `compute-address`

Look up the configured address for a contract.

```bash
ev-deployer compute-address --config deploy.toml --contract permit2
```

## Config Reference

### `[chain]`

| Field | Type | Description |
|-------|------|-------------|
| `chain_id` | u64 | Chain ID |

### `[contracts.admin_proxy]`

| Field | Type | Description |
|-------|------|-------------|
| `address` | address | Address to deploy at (required for genesis, ignored for deploy) |
| `owner` | address | Owner address (must not be zero) |

### `[contracts.permit2]`

| Field | Type | Description |
|-------|------|-------------|
| `address` | address | Address to deploy at (canonical: `0x000000000022D473030F116dDEE9F6B43aC78BA3`). Required for genesis, ignored for deploy. |

### `[contracts.deterministic_deployer]`

| Field | Type | Description |
|-------|------|-------------|
| `address` | address | Address (canonical: `0x4e59b44847b379578588920cA78FbF26c0B4956C`). Required for genesis. Genesis-only — not used in deploy mode. |

## Contracts

| Contract | Description |
|----------|-------------|
| `admin_proxy` | Transparent proxy with owner-based access control |
| `permit2` | Uniswap canonical token approval manager (same address on all chains) |
| `deterministic_deployer` | Nick's CREATE2 factory — genesis-only, needed on post-merge chains |

Runtime bytecodes are embedded in the binary — no external toolchain is needed at deploy time.

## Testing

```bash
just test-deployer
```
