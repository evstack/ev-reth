/**
 * Basic example: Send an EvNode transaction
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
  // Create a standard viem client
  const client = createClient({
    transport: http(RPC_URL),
  });

  // Create account from private key
  const account = privateKeyToAccount(PRIVATE_KEY);

  // Create EvNode client with executor signer
  const evnode = createEvnodeClient({
    client,
    executor: {
      address: account.address,
      signHash: async (hash) => sign({ hash, privateKey: PRIVATE_KEY }),
    },
  });

  console.log('Sending EvNode transaction from:', account.address);

  // Send a simple self-transfer
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
