# ev-dev

One-command local development chain for Evolve. Think of it as the Evolve equivalent of [Hardhat Node](https://hardhat.org/hardhat-network/docs/overview) or [Anvil](https://book.getfoundry.sh/reference/anvil/).

## Installation

```bash
# Install to ~/.cargo/bin
just install-ev-dev

# Or build without installing
just build-ev-dev
```

## Quick Start

```bash
# Build and run
just dev-chain

# Or run directly after installing
ev-dev
```

The chain starts immediately with 10 pre-funded accounts, each holding 1,000,000 ETH.

## CLI Options

```
ev-dev [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--host` | `127.0.0.1` | Host to bind HTTP/WS RPC server |
| `--port` | `8545` | Port for HTTP/WS RPC server |
| `--block-time` | `1` | Block time in seconds (`0` = mine on transaction) |
| `--silent` | `false` | Suppress the startup banner |
| `--accounts` | `10` | Number of accounts to display (1-20) |
| `--deploy-config` | — | Path to an ev-deployer TOML config to deploy contracts at genesis |
| `--tui` | `false` | Launch with an interactive terminal UI instead of plain log output |

### TUI Mode

Pass `--tui` to launch an interactive terminal dashboard:

```bash
ev-dev --tui
```

The TUI shows:

- **Chain info** — chain ID, RPC URL, block time
- **Accounts** — addresses, private keys, and real-time balances (polled every 2s)
- **Deployed contracts** — when using `--deploy-config`
- **Logs** — live node logs with scrollback

Keyboard shortcuts:

| Key | Action |
|-----|--------|
| `Tab` | Cycle between panels |
| `↑` / `↓` | Scroll within the active panel |
| `q` / `Esc` / `Ctrl+C` | Quit |

### Examples

```bash
# Mine blocks only when transactions arrive
ev-dev --block-time 0

# Listen on all interfaces (useful inside Docker/VMs)
ev-dev --host 0.0.0.0

# Custom port, faster blocks
ev-dev --port 9545 --block-time 2

# Start with genesis contracts deployed
ev-dev --deploy-config bin/ev-deployer/examples/devnet.toml
```

## Genesis Contract Deployment

You can deploy contracts into the genesis state by passing a `--deploy-config` flag pointing to an [ev-deployer](../ev-deployer/README.md) TOML config file.

```bash
ev-dev --deploy-config path/to/deploy.toml
```

When a deploy config is provided, ev-dev will:

1. Load and validate the config
2. Override the config's `chain_id` to match the devnet genesis (a warning is printed if they differ)
3. Merge the contract alloc entries into the genesis state before starting the node
4. Print the deployed contract addresses in the startup banner

The startup banner will show an extra section:

```
Genesis Contracts (from path/to/deploy.toml)
==================
  admin_proxy          "0x000000000000000000000000000000000000Ad00"
  fee_vault            "0x000000000000000000000000000000000000FE00"
  ...
```

See the [ev-deployer README](../ev-deployer/README.md) for full config reference and available contracts.

## Live Contract Deployment (CREATE2)

You can also deploy contracts to a running ev-dev chain using `ev-deployer deploy`. This uses the [deterministic deployer](https://github.com/Arachnid/deterministic-deployment-proxy) (Nick's CREATE2 factory at `0x4e59b44847b379578588920ca78fbf26c0b4956c`), which is pre-included in the devnet genesis.

```bash
# Terminal 1: start the chain
ev-dev

# Terminal 2: deploy contracts via CREATE2
ev-deployer deploy \
    --config bin/ev-deployer/examples/devnet.toml \
    --rpc-url http://127.0.0.1:8545 \
    --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
    --state /tmp/deploy-state.json
```

In deploy mode, contract addresses are computed deterministically via CREATE2 (the `address` field in the config is ignored). The `--state` file tracks progress and allows resuming interrupted deployments.

**Genesis vs Deploy mode**: Use `--deploy-config` (genesis mode) when you want contracts available from block 0 with exact addresses. Use `ev-deployer deploy` when you want to test the deployment pipeline itself or need CREATE2-derived addresses.

## Chain Details

| Property | Value |
|----------|-------|
| Chain ID | `1234` |
| Gas limit | 30,000,000 |
| Base fee | 1 Gwei |
| Contract size limit | 128 KB |
| Hardforks | All enabled at genesis (through Cancun) |

## Pre-funded Accounts

Accounts are derived from the standard Hardhat mnemonic:

```
test test test test test test test test test test test junk
```

Derivation path: `m/44'/60'/0'/0/{index}`

| # | Address | Private Key |
|---|---------|-------------|
| 0 | `0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266` | `0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80` |
| 1 | `0x70997970C51812dc3A010C7d01b50e0d17dc79C8` | `0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d` |
| 2 | `0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC` | `0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a` |
| 3 | `0x90F79bf6EB2c4f870365E785982E1f101E93b906` | `0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6` |
| 4 | `0x15d34AAf54267DB7D7c367839AAf71A00a2C6A65` | `0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a` |
| 5 | `0x9965507D1a55bcC2695C58ba16FB37d819B0A4dc` | `0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba` |
| 6 | `0x976EA74026E726554dB657fA54763abd0C3a0aa9` | `0x92db14e403b83dfe3df233f83dfa3a0d7096f21ca9b0d6d6b8d88b2b4ec1564e` |
| 7 | `0x14dC79964da2C08dba798bBb5d93A585CAa97F90` | `0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356` |
| 8 | `0x23618e81E3f5cdF7f54C3d65f7FBc0aBf5B21E8f` | `0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97` |
| 9 | `0xa0Ee7A142d267C1f36714E4a8F75612F20a79720` | `0x2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6` |

> **WARNING**: These accounts and their private keys are publicly known. Any funds sent to them on a real network **will be lost**.

## Using with Common Tools

### Foundry (cast / forge)

```bash
# Check balance
cast balance 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266 --rpc-url http://127.0.0.1:8545

# Send ETH
cast send 0x70997970C51812dc3A010C7d01b50e0d17dc79C8 \
  --value 1ether \
  --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80 \
  --rpc-url http://127.0.0.1:8545

# Deploy a contract
forge create src/MyContract.sol:MyContract \
  --rpc-url http://127.0.0.1:8545 \
  --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80

# Run Forge tests against ev-dev
forge test --fork-url http://127.0.0.1:8545
```

### Hardhat

In `hardhat.config.js`:

```js
module.exports = {
  networks: {
    evdev: {
      url: "http://127.0.0.1:8545",
      chainId: 1234,
      accounts: [
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
        "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
      ],
    },
  },
};
```

```bash
npx hardhat run scripts/deploy.js --network evdev
```

### ethers.js / viem

```js
// ethers.js v6
import { JsonRpcProvider, Wallet } from "ethers";

const provider = new JsonRpcProvider("http://127.0.0.1:8545");
const wallet = new Wallet(
  "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
  provider
);
```

```js
// viem
import { createWalletClient, http } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { defineChain } from "viem";

const evdev = defineChain({
  id: 1234,
  name: "ev-dev",
  nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
  rpcUrls: {
    default: { http: ["http://127.0.0.1:8545"] },
  },
});

const account = privateKeyToAccount(
  "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
);
const client = createWalletClient({ account, chain: evdev, transport: http() });
```

### curl (raw JSON-RPC)

```bash
# Get block number
curl -s http://127.0.0.1:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'

# Get chain ID
curl -s http://127.0.0.1:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_chainId","params":[],"id":1}'

# Get pending transactions from txpool
curl -s http://127.0.0.1:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"txpoolExt_getTxs","params":[],"id":1}'
```

## Available RPC Namespaces

The following namespaces are enabled by default over both HTTP and WebSocket:

- `eth` — standard Ethereum JSON-RPC
- `net` — network info
- `web3` — client version, SHA3
- `txpool` — transaction pool inspection
- `debug` — debug namespace (traceCall, traceTransaction, etc.)
- `trace` — OpenEthereum-compatible trace namespace

Additionally, the custom `txpoolExt` namespace is available:

- `txpoolExt_getTxs` — returns pending transactions as RLP-encoded bytes

## Evolve-specific Features

ev-dev includes all Evolve customizations out of the box:

- **Base fee redirect**: Base fees are sent to `0x00...00fe` instead of being burned
- **128 KB contract size limit**: Deploy contracts up to 128 KB (vs Ethereum's 24 KB)
- **Mint precompile**: Native minting precompile is active, admin is account `0` (`0xf39F...2266`)
- **EvNode transactions (type 0x76)**: Batch calls and sponsored transactions are supported

## How It Works

ev-dev is a thin wrapper around the full `ev-reth` node. On startup it:

1. If `--deploy-config` is provided, loads the config and merges contract alloc entries into the genesis
2. Writes the (possibly extended) devnet genesis to a temp file
3. Creates a temporary data directory (clean state every run)
4. Launches `ev-reth` in `--dev` mode with networking disabled
5. Exposes HTTP and WebSocket RPC on the configured host/port

Each run starts from a fresh genesis — there is no persistent state between restarts.
