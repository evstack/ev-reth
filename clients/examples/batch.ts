/**
 * Batch example: Send multiple calls in a single transaction
 *
 * Run with:
 *   PRIVATE_KEY=0x... npx tsx examples/batch.ts
 */
import { createClient, http, formatEther } from 'viem';
import { privateKeyToAccount, sign } from 'viem/accounts';
import { createEvnodeClient } from '../src/index.ts';

const RPC_URL = process.env.RPC_URL ?? 'http://localhost:8545';
const PRIVATE_KEY = process.env.PRIVATE_KEY as `0x${string}`;

if (!PRIVATE_KEY) {
  console.error('Usage: PRIVATE_KEY=0x... npx tsx examples/batch.ts');
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

  // Example recipients (in practice, use real addresses)
  const recipient1 = '0x1111111111111111111111111111111111111111' as const;
  const recipient2 = '0x2222222222222222222222222222222222222222' as const;
  const amount = 1000000000000000n; // 0.001 ETH

  console.log(`\nSending ${formatEther(amount)} ETH to each recipient...`);

  // Send batch transaction: multiple transfers in one tx
  const txHash = await evnode.send({
    calls: [
      { to: recipient1, value: amount, data: '0x' },
      { to: recipient2, value: amount, data: '0x' },
    ],
  });

  console.log('Transaction hash:', txHash);
  console.log('\nBoth transfers executed atomically in a single transaction.');
}

main().catch(console.error);
