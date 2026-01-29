/**
 * Manual params example: Specify gas, nonce, and fees manually
 *
 * Run with:
 *   PRIVATE_KEY=0x... npx tsx examples/manual-params.ts
 */
import { createClient, http } from 'viem';
import { privateKeyToAccount, sign } from 'viem/accounts';
import { createEvnodeClient } from '../src/index.ts';

const RPC_URL = process.env.RPC_URL ?? 'http://localhost:8545';
const PRIVATE_KEY = process.env.PRIVATE_KEY as `0x${string}`;

if (!PRIVATE_KEY) {
  console.error('Usage: PRIVATE_KEY=0x... npx tsx examples/manual-params.ts');
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

  // Get current nonce
  const nonce = await client.request({
    method: 'eth_getTransactionCount',
    params: [account.address, 'pending'],
  });

  console.log('\nCurrent nonce:', nonce);

  // Send with manual parameters
  const txHash = await evnode.send({
    calls: [
      {
        to: account.address,
        value: 0n,
        data: '0x',
      },
    ],
    // Manual overrides
    nonce: BigInt(nonce as string),
    gasLimit: 100000n,
    maxFeePerGas: 1000000000n, // 1 gwei
    maxPriorityFeePerGas: 0n,
    accessList: [],
  });

  console.log('Transaction hash:', txHash);
}

main().catch(console.error);
