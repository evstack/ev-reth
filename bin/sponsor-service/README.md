# Sponsor Service

Transparent JSON-RPC proxy that sponsors gas fees for ev-reth transactions. Clients point their RPC URL to this service instead of the node — the service intercepts `eth_sendRawTransaction` for type `0x76` transactions, adds the sponsor signature, and forwards to the real node. All other RPC calls are proxied transparently.

## How it works

```
Client (evnode-viem)       Sponsor Service                 ev-reth
     |                              |                            |
     | eth_sendRawTransaction(0x76) |                            |
     |----------------------------->|                            |
     |                              | 1. Decode 0x76 tx          |
     |                              | 2. No feePayerSignature?   |
     |                              | 3. Validate policy         |
     |                              | 4. Sign as sponsor         |
     |                              | 5. Re-encode & forward --->|
     |                              |                            |
     |    { txHash }                |<---------------------------|
     |<-----------------------------|                            |
     |                              |                            |
     | eth_chainId / eth_getBalance |                            |
     |----------------------------->| proxy ---------------------->|
     |<-----------------------------|<----------------------------|
```

The client needs zero code changes — just change the RPC URL from the node to the sponsor service.

## Setup

Requires [Bun](https://bun.sh) runtime.

```bash
# Build the client library first
cd ../../clients && bun install && bun run build

# Install dependencies
cd ../../bin/sponsor-service && bun install
```

## Configuration

All config via environment variables:

| Variable | Required | Default | Description |
|---|---|---|---|
| `RPC_URL` | Yes | - | ev-reth JSON-RPC endpoint |
| `CHAIN_ID` | Yes | - | Chain ID to accept |
| `SPONSOR_PRIVATE_KEY` | Yes | - | Hex-encoded private key for the sponsor account |
| `MAX_GAS_LIMIT_PER_TX` | No | `500000` | Max gas limit per sponsored tx |
| `MAX_FEE_PER_GAS_LIMIT` | No | `100000000000` (100 gwei) | Max fee per gas allowed |
| `MIN_SPONSOR_BALANCE` | No | `1000000000000000000` (1 ETH) | Min sponsor balance to accept txs |
| `PORT` | No | `3000` | HTTP port |

## Running

```bash
RPC_URL=http://localhost:8545 \
CHAIN_ID=1337 \
SPONSOR_PRIVATE_KEY=0x... \
bun run start
```

For development with auto-reload:

```bash
bun run dev
```

## Usage

Point your `@evstack/evnode-viem` client to the sponsor service instead of the node:

```typescript
import { createClient, http } from 'viem';
import { createEvnodeClient } from '@evstack/evnode-viem';

const client = createClient({ transport: http('http://localhost:3000') }); // sponsor service
const evnode = createEvnodeClient({
  client,
  executor: { address: myAddress, signHash: mySignFn },
});

// Works exactly as if talking to the node — sponsor pays gas
const txHash = await evnode.send({ calls: [{ to, value, data }] });
```

## API

### JSON-RPC proxy (`POST /`)

All JSON-RPC requests are accepted at the root endpoint. `eth_sendRawTransaction` with type `0x76` transactions that lack a `feePayerSignature` are intercepted, sponsored, and forwarded. Everything else is proxied to the upstream node.

Policy violations return standard JSON-RPC errors:
- `-32602` — `CHAIN_ID_MISMATCH`, `GAS_LIMIT_EXCEEDED`, `FEE_TOO_HIGH`, `INVALID_INTENT`
- `-32003` — `SPONSOR_BALANCE_LOW`, `NODE_ERROR`

### REST endpoints

| Endpoint | Description |
|---|---|
| `GET /v1/health` | Health check with sponsor balance and node connectivity |
| `GET /v1/policy` | Current sponsorship policies |

## Tests

```bash
bun test              # all tests
bun test tests/unit   # unit only
bun test tests/integration  # integration only
```

### E2E testing

Requires a running ev-reth chain (e.g. via docker compose from ev-node) and the sponsor service:

```bash
# 1. Start the chain
cd /path/to/ev-node/apps/evm && docker compose up -d

# 2. Fund the executor (txpool requires some balance for tx promotion)
bun run tests/e2e/fund-executor.ts

# 3. Start the sponsor service
RPC_URL=http://localhost:8545 CHAIN_ID=1234 SPONSOR_PRIVATE_KEY=0x... bun run start

# 4. Run the E2E test
bun run tests/e2e/sponsor-e2e.ts
```
