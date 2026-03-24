# EV Deployer

CLI tool for generating genesis alloc entries for ev-reth contracts. It reads a declarative TOML config and produces the JSON needed to embed contracts into a chain's genesis state.

## Building

```bash
just build-deployer
```

The binary is output to `target/release/ev-deployer`.

## Configuration

EV Deployer uses a TOML config file to define what contracts to include and how to configure them. See [`examples/devnet.toml`](examples/devnet.toml) for a complete example.

See [`examples/devnet.toml`](examples/devnet.toml) for a complete example with all contracts configured.

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

| Field                | Type    | Description                                      |
|----------------------|---------|--------------------------------------------------|
| `address`            | address | Address to deploy at                             |
| `owner`              | address | Owner address                                    |
| `destination_domain` | u32     | Hyperlane destination domain (default: 0)        |
| `recipient_address`  | bytes32 | Hyperlane recipient address (default: zero)      |
| `minimum_amount`     | u64     | Minimum amount for bridging (default: 0)         |
| `call_fee`           | u64     | Call fee for sendToCelestia (default: 0)         |
| `bridge_share_bps`   | u64     | Basis points for bridge share, 0–10000 (default: 0, treated as 10000) |
| `other_recipient`    | address | Other recipient for split accounting (default: zero) |
| `hyp_native_minter`  | address | HypNativeMinter address (default: zero)          |

#### `[contracts.mailbox]`

| Field           | Type    | Description                                         |
|-----------------|---------|-----------------------------------------------------|
| `address`       | address | Address to deploy at                                |
| `owner`         | address | Owner address (default: zero)                       |
| `default_ism`   | address | Default interchain security module (default: zero)  |
| `default_hook`  | address | Default post-dispatch hook (default: zero)          |
| `required_hook` | address | Required post-dispatch hook, e.g. MerkleTreeHook (default: zero) |

#### `[contracts.merkle_tree_hook]`

| Field     | Type    | Description                                        |
|-----------|---------|----------------------------------------------------|
| `address` | address | Address to deploy at                               |
| `owner`   | address | Owner address (default: zero)                      |
| `mailbox` | address | Mailbox address (patched into bytecode as immutable)|

#### `[contracts.noop_ism]`

| Field     | Type    | Description          |
|-----------|---------|----------------------|
| `address` | address | Address to deploy at |

#### `[contracts.protocol_fee]`

| Field              | Type    | Description                                       |
|--------------------|---------|---------------------------------------------------|
| `address`          | address | Address to deploy at                              |
| `owner`            | address | Owner address (default: zero)                     |
| `max_protocol_fee` | u64     | Maximum protocol fee in wei                       |
| `protocol_fee`     | u64     | Protocol fee charged per dispatch in wei (default: 0) |
| `beneficiary`      | address | Beneficiary address that receives collected fees (default: zero) |

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
  "fee_vault": "0x000000000000000000000000000000000000FE00",
  "mailbox": "0x0000000000000000000000000000000000001200",
  "merkle_tree_hook": "0x0000000000000000000000000000000000001100",
  "noop_ism": "0x0000000000000000000000000000000000001300",
  "protocol_fee": "0x0000000000000000000000000000000000001400"
}
```

### Look up a contract address

```bash
ev-deployer compute-address --config deploy.toml --contract admin_proxy
```

## Contracts

| Contract           | Description                                             |
|--------------------|---------------------------------------------------------|
| `admin_proxy`      | Proxy contract with owner-based access control          |
| `fee_vault`        | Fee vault with Hyperlane bridging support               |
| `mailbox`          | Hyperlane core messaging hub                            |
| `merkle_tree_hook` | Hyperlane required hook (Merkle tree for messages)      |
| `noop_ism`         | Hyperlane ISM that accepts all messages                 |
| `protocol_fee`     | Hyperlane post-dispatch hook that charges a protocol fee|

Runtime bytecodes are embedded in the binary — no external toolchain is needed at deploy time.

## Testing

```bash
just test-deployer
```
