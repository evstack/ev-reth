/**
 * Contract call example: Interact with a smart contract
 *
 * Run with:
 *   PRIVATE_KEY=0x... CONTRACT=0x... npx tsx examples/contract-call.ts
 */
import { createClient, http, encodeFunctionData, parseAbi } from 'viem';
import { privateKeyToAccount, sign } from 'viem/accounts';
import { createEvnodeClient } from '../src/index.ts';

const RPC_URL = process.env.RPC_URL ?? 'http://localhost:8545';
const PRIVATE_KEY = process.env.PRIVATE_KEY as `0x${string}`;
const CONTRACT = process.env.CONTRACT as `0x${string}`;

if (!PRIVATE_KEY) {
  console.error('Usage: PRIVATE_KEY=0x... CONTRACT=0x... npx tsx examples/contract-call.ts');
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

  // Example: ERC20 transfer
  // In practice, replace with your contract's ABI and function
  const abi = parseAbi([
    'function transfer(address to, uint256 amount) returns (bool)',
  ]);

  const data = encodeFunctionData({
    abi,
    functionName: 'transfer',
    args: ['0x1111111111111111111111111111111111111111', 1000000n],
  });

  const contractAddress = CONTRACT ?? '0x0000000000000000000000000000000000000000';

  console.log('\nCalling contract:', contractAddress);

  const txHash = await evnode.send({
    calls: [
      {
        to: contractAddress,
        value: 0n,
        data,
      },
    ],
  });

  console.log('Transaction hash:', txHash);
}

main().catch(console.error);
