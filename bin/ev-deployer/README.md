# EV Deployer

CLI tool for generating genesis alloc entries for ev-reth contracts. It reads a declarative TOML config and produces the JSON needed to embed contracts into a chain's genesis state.

## Building

```bash
just build-deployer
```

The binary is output to `target/release/ev-deployer`.

## Configuration

EV Deployer uses a TOML config file to define what contracts to include and how to configure them. See [`examples/devnet.toml`](examples/devnet.toml) for a complete example.

```toml
[chain]
chain_id = 1234

[contracts]
```

### Config reference

#### `[chain]`

| Field      | Type | Description |
|------------|------|-------------|
| `chain_id` | u64  | Chain ID    |

## Usage

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

## Testing

```bash
just test-deployer
```
