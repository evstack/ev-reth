/**
 * Contract deploy example: Deploy a smart contract
 *
 * Run with:
 *   PRIVATE_KEY=0x... npx tsx examples/contract-deploy.ts
 */
import { createClient, http, type Hex } from 'viem';
import { privateKeyToAccount, sign } from 'viem/accounts';
import { createEvnodeClient } from '../src/index.ts';

const RPC_URL = process.env.RPC_URL ?? 'http://localhost:8545';
const PRIVATE_KEY = process.env.PRIVATE_KEY as `0x${string}`;

if (!PRIVATE_KEY) {
  console.error('Usage: PRIVATE_KEY=0x... npx tsx examples/contract-deploy.ts');
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

  // Simple storage contract bytecode
  // contract Storage { uint256 value; function set(uint256 v) { value = v; } function get() view returns (uint256) { return value; } }
  const bytecode: Hex = '0x608060405234801561001057600080fd5b5060df8061001f6000396000f3fe6080604052348015600f57600080fd5b5060043610603c5760003560e01c80636d4ce63c1460415780638a42ebe914605b575b600080fd5b60005460405190815260200160405180910390f35b606b6066366004606d565b600055565b005b600060208284031215607e57600080fd5b503591905056fea264697066735822122041c7f6d2d7b0d1c0d6c0d8e7f4c5b3a2918d7e6f5c4b3a291807d6e5f4c3b2a164736f6c63430008110033';

  console.log('\nDeploying contract...');

  // Deploy with to=null (CREATE)
  const txHash = await evnode.send({
    calls: [
      {
        to: null,
        value: 0n,
        data: bytecode,
      },
    ],
  });

  console.log('Transaction hash:', txHash);
  console.log('\nContract deployed. Check receipt for contract address.');
}

main().catch(console.error);
