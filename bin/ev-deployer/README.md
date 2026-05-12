# EV Deployer

CLI tool for generating genesis alloc entries for ev-reth contracts. It reads a declarative TOML config and produces the JSON needed to embed contracts into a chain's genesis state.

## Building

```bash
just build-deployer
```

The binary is output to `target/release/ev-deployer`.

## Configuration

EV Deployer uses a TOML config file to define what contracts to include and how to configure them. See [`examples/devnet.toml`](examples/devnet.toml) for a complete example.

### Config reference

#### `[chain]`

| Field      | Type | Description |
|------------|------|-------------|
| `chain_id` | u64  | Chain ID    |

#### `[contracts.admin_proxy]`

| Field     | Type    | Description               |
|-----------|---------|---------------------------|
| `address` | address | Address to deploy at      |
| `owner`   | address | Owner (must not be zero)  |

#### `[contracts.permit2]`

| Field     | Type    | Description                                              |
|-----------|---------|----------------------------------------------------------|
| `address` | address | Address to deploy at (canonical: `0x000000000022D473030F116dDEE9F6B43aC78BA3`) |

## Usage

### Generate a starter config

```bash
ev-deployer init --output deploy.toml
```

This creates a TOML config template with all supported contracts commented out and documented.

### Generate genesis alloc

Print alloc JSON to stdout:

```bash
ev-deployer genesis --config deploy.toml
```

Write to a file:

```bash
ev-deployer genesis --config deploy.toml --output alloc.json
```

### Merge into an existing genesis file

Insert the generated entries into an existing `genesis.json`. The merged result is written to `--output` (or stdout if `--output` is omitted):

```bash
ev-deployer genesis --config deploy.toml --merge-into genesis.json --output genesis-out.json
```

If an address already exists in the genesis, the command fails. Use `--force` to overwrite:

```bash
ev-deployer genesis --config deploy.toml --merge-into genesis.json --output genesis-out.json --force
```

### Export address manifest

Write a JSON mapping of contract names to their configured addresses:

```bash
ev-deployer genesis --config deploy.toml --addresses-out addresses.json
```

Output:

```json
{
  "admin_proxy": "0x000000000000000000000000000000000000Ad00",
  "permit2": "0x000000000022D473030F116dDEE9F6B43aC78BA3"
}
```

### Look up a contract address

```bash
ev-deployer compute-address --config deploy.toml --contract admin_proxy
```

## Contracts

| Contract      | Description                                        |
|---------------|----------------------------------------------------|
| `admin_proxy` | Proxy contract with owner-based access control     |
| `permit2`     | Uniswap canonical token approval manager           |

Runtime bytecodes are embedded in the binary — no external toolchain is needed at deploy time.

## Testing

```bash
just test-deployer
```
