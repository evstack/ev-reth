/**
 * Sponsored example: Sponsor pays gas on behalf of executor
 *
 * Run with:
 *   EXECUTOR_KEY=0x... SPONSOR_KEY=0x... npx tsx examples/sponsored.ts
 */
import { createClient, http, formatEther, type Hex } from 'viem';
import { privateKeyToAccount, sign } from 'viem/accounts';
import { createEvnodeClient } from '../src/index.ts';

const RPC_URL = process.env.RPC_URL ?? 'http://localhost:8545';
const EXECUTOR_KEY = process.env.EXECUTOR_KEY as `0x${string}`;
const SPONSOR_KEY = process.env.SPONSOR_KEY as `0x${string}`;

if (!EXECUTOR_KEY || !SPONSOR_KEY) {
  console.error('Usage: EXECUTOR_KEY=0x... SPONSOR_KEY=0x... npx tsx examples/sponsored.ts');
  process.exit(1);
}

async function main() {
  const client = createClient({ transport: http(RPC_URL) });

  const executorAccount = privateKeyToAccount(EXECUTOR_KEY);
  const sponsorAccount = privateKeyToAccount(SPONSOR_KEY);

  console.log('Executor:', executorAccount.address);
  console.log('Sponsor:', sponsorAccount.address);

  const evnode = createEvnodeClient({
    client,
    executor: {
      address: executorAccount.address,
      signHash: async (hash) => sign({ hash, privateKey: EXECUTOR_KEY }),
    },
    sponsor: {
      address: sponsorAccount.address,
      signHash: async (hash) => sign({ hash, privateKey: SPONSOR_KEY }),
    },
  });

  // Step 1: Executor creates an intent (signs the transaction)
  console.log('\n1. Creating intent (executor signs)...');
  const intent = await evnode.createIntent({
    calls: [
      {
        to: executorAccount.address,
        value: 0n,
        data: '0x',
      },
    ],
  });

  console.log('   Intent created with executor signature');

  // Step 2: Sponsor signs and sends the transaction
  console.log('\n2. Sponsor signs and sends...');
  const txHash = await evnode.sponsorAndSend({ intent });

  console.log('Transaction hash:', txHash);
  console.log('\nThe sponsor paid the gas fees, executor paid nothing.');
}

main().catch(console.error);
