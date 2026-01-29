/**
 * Basic example: Send a simple EvNode transaction
 *
 * Run with:
 *   PRIVATE_KEY=0x... npx tsx examples/basic.ts
 */
import { createClient, http } from 'viem';
import { privateKeyToAccount, sign } from 'viem/accounts';
import { createEvnodeClient } from '../src/index.ts';

const RPC_URL = process.env.RPC_URL ?? 'http://localhost:8545';
const PRIVATE_KEY = process.env.PRIVATE_KEY as `0x${string}`;

if (!PRIVATE_KEY) {
  console.error('Usage: PRIVATE_KEY=0x... npx tsx examples/basic.ts');
  process.exit(1);
}

async function main() {
  const client = createClient({ transport: http(RPC_URL) });
  const account = privateKeyToAccount(PRIVATE_KEY);

  const evnode = createEvnodeClient({
    client,
    executor: {
      address: account.address,
      signHash: async (hash) => sign({ hash, privateKey: PRIVATE_KEY }),
    },
  });

  console.log('Executor:', account.address);

  // Send a simple transaction (self-transfer with no value)
  const txHash = await evnode.send({
    calls: [
      {
        to: account.address,
        value: 0n,
        data: '0x',
      },
    ],
  });

  console.log('Transaction hash:', txHash);
}

main().catch(console.error);
