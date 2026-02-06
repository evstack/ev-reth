# @evstack/evnode-viem

Viem client extension for EvNode transactions (type 0x76).

## Installation

```bash
npm install @evstack/evnode-viem viem
```

## Usage

### Basic Transaction

```typescript
import { createClient, http } from 'viem';
import { privateKeyToAccount, sign } from 'viem/accounts';
import { createEvnodeClient } from '@evstack/evnode-viem';

const client = createClient({
  transport: http('http://localhost:8545'),
});

const account = privateKeyToAccount('0x...');

const evnode = createEvnodeClient({
  client,
  executor: {
    address: account.address,
    signHash: async (hash) => sign({ hash, privateKey: '0x...' }),
  },
});

// Send a transaction
const txHash = await evnode.send({
  calls: [
    { to: '0x...', value: 0n, data: '0x' },
  ],
});
```

### Batch Transactions

EvNode transactions support multiple calls in a single transaction:

```typescript
const txHash = await evnode.send({
  calls: [
    { to: recipient1, value: 1000000000000000n, data: '0x' },
    { to: recipient2, value: 1000000000000000n, data: '0x' },
  ],
});
```

### Sponsored Transactions

A sponsor can pay gas fees on behalf of the executor:

```typescript
const evnode = createEvnodeClient({
  client,
  executor: { address: executorAddr, signHash: executorSignFn },
  sponsor: { address: sponsorAddr, signHash: sponsorSignFn },
});

// Create intent (signed by executor)
const intent = await evnode.createIntent({
  calls: [{ to: '0x...', value: 0n, data: '0x' }],
});

// Sponsor signs and sends
const txHash = await evnode.sponsorAndSend({ intent });
```

## API

### `createEvnodeClient(options)`

Creates a new EvNode client.

**Options:**
- `client` - Viem Client instance
- `executor` - (optional) Default executor signer
- `sponsor` - (optional) Default sponsor signer

### Client Methods

- `send(args)` - Sign and send an EvNode transaction
- `createIntent(args)` - Create a sponsorable intent
- `sponsorIntent(args)` - Add sponsor signature to an intent
- `sponsorAndSend(args)` - Sponsor and send in one call
- `serialize(signedTx)` - Serialize a signed transaction
- `deserialize(hex)` - Deserialize a signed transaction

### Utility Functions

- `computeExecutorSigningHash(tx)` - Get hash for executor to sign
- `computeSponsorSigningHash(tx, executorAddress)` - Get hash for sponsor to sign
- `computeTxHash(signedTx)` - Get transaction hash
- `recoverExecutor(signedTx)` - Recover executor address from signature
- `recoverSponsor(tx, executorAddress)` - Recover sponsor address from signature
- `estimateIntrinsicGas(calls)` - Estimate minimum gas for calls

## License

MIT
