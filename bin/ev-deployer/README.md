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

[contracts.admin_proxy]
address = "0x000000000000000000000000000000000000Ad00"
owner = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"

[contracts.fee_vault]
address = "0x000000000000000000000000000000000000FE00"
owner = "0x000000000000000000000000000000000000Ad00"
destination_domain = 0
recipient_address = "0x0000000000000000000000000000000000000000000000000000000000000000"
minimum_amount = 0
call_fee = 0
bridge_share_bps = 10000
other_recipient = "0x0000000000000000000000000000000000000000"
hyp_native_minter = "0x0000000000000000000000000000000000000000"
```

Both contracts are optional — include only the sections you need.

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

#### `[contracts.fee_vault]`

| Field                | Type    | Default | Description                                    |
|----------------------|---------|---------|------------------------------------------------|
| `address`            | address | —       | Address to deploy at                           |
| `owner`              | address | —       | Owner (must not be zero)                       |
| `destination_domain` | u32     | 0       | Hyperlane destination domain                   |
| `recipient_address`  | bytes32 | 0x0…0   | Hyperlane recipient                            |
| `minimum_amount`     | u64     | 0       | Minimum amount for bridging                    |
| `call_fee`           | u64     | 0       | Fee for sendToCelestia                         |
| `bridge_share_bps`   | u64     | 0       | Bridge share in basis points (0–10000). 0 maps to 10000 |
| `other_recipient`    | address | 0x0…0   | Split accounting recipient                     |
| `hyp_native_minter`  | address | 0x0…0   | HypNativeMinter address                        |

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

Output:

```json
{
  "admin_proxy": "0x000000000000000000000000000000000000Ad00",
  "fee_vault": "0x000000000000000000000000000000000000FE00"
}
```

### Look up a contract address

```bash
ev-deployer compute-address --config deploy.toml --contract admin_proxy
```

## Contracts

| Contract       | Description                                         |
|----------------|-----------------------------------------------------|
| `admin_proxy`  | Proxy contract with owner-based access control      |
| `fee_vault`    | Fee vault with Hyperlane bridge integration          |

Runtime bytecodes are embedded in the binary — no external toolchain is needed at deploy time.

## Testing

```bash
just test-deployer
```
