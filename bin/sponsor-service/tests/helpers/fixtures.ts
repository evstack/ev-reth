import { privateKeyToAccount } from 'viem/accounts';
import { parseSignature } from 'viem';
import type { HashSigner, EvNodeTransaction, SponsorableIntent } from '@evstack/evnode-viem';
import { signAsExecutor } from '@evstack/evnode-viem';
import type { SponsorConfig } from '../../src/config.js';

export const TEST_EXECUTOR_KEY = '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80' as const; // gitleaks:allow
export const TEST_SPONSOR_KEY = '0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d' as const; // gitleaks:allow
export const TEST_CHAIN_ID = 1337n;

export function makeConfig(overrides: Partial<SponsorConfig> = {}): SponsorConfig {
  return {
    rpcUrl: 'http://localhost:8545',
    chainId: TEST_CHAIN_ID,
    sponsorPrivateKey: TEST_SPONSOR_KEY,
    maxGasLimitPerTx: 500_000n,
    maxFeePerGasLimit: 100_000_000_000n,
    minSponsorBalance: 1_000_000_000_000_000_000n,
    port: 3000,
    ...overrides,
  };
}

export function makeTx(overrides: Partial<EvNodeTransaction> = {}): EvNodeTransaction {
  return {
    chainId: TEST_CHAIN_ID,
    nonce: 0n,
    maxPriorityFeePerGas: 1_000_000_000n,
    maxFeePerGas: 10_000_000_000n,
    gasLimit: 21_000n,
    calls: [{ to: '0x70997970C51812dc3A010C7d01b50e0d17dc79C8', value: 0n, data: '0x' }],
    accessList: [],
    ...overrides,
  };
}

export function makeHashSigner(): HashSigner {
  const account = privateKeyToAccount(TEST_EXECUTOR_KEY);
  return {
    address: account.address,
    signHash: async (hash) => {
      const sig = await account.sign({ hash });
      return parseSignature(sig);
    },
  };
}

export async function makeIntent(tx?: EvNodeTransaction): Promise<SponsorableIntent> {
  const signer = makeHashSigner();
  const transaction = tx || makeTx();
  const executorSignature = await signAsExecutor(transaction, signer);
  return { tx: transaction, executorSignature, executorAddress: signer.address };
}
