import {
  type Address,
  type Client,
  type Hex,
  type Signature,
  hexToSignature,
  isHex,
  recoverAddress,
} from 'viem';

import type { EvNodeTransaction, EvNodeSignedTransaction, HashSigner } from './types.js';
import {
  computeExecutorSigningHash,
  computeSponsorSigningHash,
  normalizeSignature,
} from './encoding.js';

export async function recoverExecutor(
  signedTx: EvNodeSignedTransaction,
): Promise<Address> {
  const hash = computeExecutorSigningHash(signedTx.transaction);
  return recoverAddress({ hash, signature: normalizeSignature(signedTx.executorSignature) });
}

export async function recoverSponsor(
  tx: EvNodeTransaction,
  executorAddress: Address,
): Promise<Address | null> {
  if (!tx.feePayerSignature) return null;
  const hash = computeSponsorSigningHash(tx, executorAddress);
  return recoverAddress({ hash, signature: normalizeSignature(tx.feePayerSignature) });
}

export async function signAsExecutor(
  tx: EvNodeTransaction,
  signer: HashSigner,
): Promise<Signature> {
  const hash = computeExecutorSigningHash(tx);
  return signer.signHash(hash);
}

export async function signAsSponsor(
  tx: EvNodeTransaction,
  executorAddress: Address,
  signer: HashSigner,
): Promise<Signature> {
  const hash = computeSponsorSigningHash(tx, executorAddress);
  return signer.signHash(hash);
}

export function hashSignerFromRpcClient(
  client: Client,
  address: Address,
): HashSigner {
  return {
    address,
    signHash: async (hash) => {
      // eth_sign is expected to sign raw bytes (no EIP-191 prefix).
      const signature = await client.request({
        method: 'eth_sign',
        params: [address, hash],
      });
      if (!isHex(signature)) {
        throw new Error('eth_sign returned non-hex signature');
      }
      return hexToSignature(signature);
    },
  };
}
