import type { Address, Hex, AccessList } from 'viem';
import { hexToBigInt, toHex } from 'viem';
import type { SponsorableIntent } from '@evstack/evnode-viem';

export interface SerializedSponsorableIntent {
  tx: {
    chainId: Hex;
    nonce: Hex;
    maxPriorityFeePerGas: Hex;
    maxFeePerGas: Hex;
    gasLimit: Hex;
    calls: Array<{ to: Address | null; value: Hex; data: Hex }>;
    accessList: AccessList;
  };
  executorSignature: { r: Hex; s: Hex; yParity: number };
  executorAddress: Address;
}

function hexSafe(value: Hex): bigint {
  return value === '0x' || value === '0x0' ? 0n : hexToBigInt(value);
}

function bigintToHex(value: bigint): Hex {
  return value === 0n ? '0x0' : toHex(value);
}

export function deserializeIntent(data: SerializedSponsorableIntent): SponsorableIntent {
  return {
    tx: {
      chainId: hexSafe(data.tx.chainId),
      nonce: hexSafe(data.tx.nonce),
      maxPriorityFeePerGas: hexSafe(data.tx.maxPriorityFeePerGas),
      maxFeePerGas: hexSafe(data.tx.maxFeePerGas),
      gasLimit: hexSafe(data.tx.gasLimit),
      calls: data.tx.calls.map((c) => ({
        to: c.to,
        value: hexSafe(c.value),
        data: c.data,
      })),
      accessList: data.tx.accessList,
    },
    executorSignature: {
      r: data.executorSignature.r,
      s: data.executorSignature.s,
      yParity: data.executorSignature.yParity,
      v: BigInt(data.executorSignature.yParity),
    },
    executorAddress: data.executorAddress,
  };
}

export function serializeIntent(intent: SponsorableIntent): SerializedSponsorableIntent {
  const sig = intent.executorSignature;
  return {
    tx: {
      chainId: bigintToHex(intent.tx.chainId),
      nonce: bigintToHex(intent.tx.nonce),
      maxPriorityFeePerGas: bigintToHex(intent.tx.maxPriorityFeePerGas),
      maxFeePerGas: bigintToHex(intent.tx.maxFeePerGas),
      gasLimit: bigintToHex(intent.tx.gasLimit),
      calls: intent.tx.calls.map((c) => ({
        to: c.to,
        value: bigintToHex(c.value),
        data: c.data,
      })),
      accessList: intent.tx.accessList,
    },
    executorSignature: {
      r: sig.r,
      s: sig.s,
      yParity: sig.yParity ?? Number(sig.v ?? 0),
    },
    executorAddress: intent.executorAddress,
  };
}
