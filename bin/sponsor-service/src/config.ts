import type { Hex } from 'viem';

export interface SponsorConfig {
  rpcUrl: string;
  chainId: bigint;
  sponsorPrivateKey: Hex;
  maxGasLimitPerTx: bigint;
  maxFeePerGasLimit: bigint;
  minSponsorBalance: bigint;
  port: number;
}

function requireEnv(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`Missing required env var: ${name}`);
  return value;
}

function envOrDefault(name: string, defaultValue: string): string {
  return process.env[name] ?? defaultValue;
}

export function loadConfig(): SponsorConfig {
  return {
    rpcUrl: requireEnv('RPC_URL'),
    chainId: BigInt(requireEnv('CHAIN_ID')),
    sponsorPrivateKey: (() => {
      const key = requireEnv('SPONSOR_PRIVATE_KEY');
      if (!/^0x[0-9a-fA-F]{64}$/.test(key)) {
        throw new Error('SPONSOR_PRIVATE_KEY must be a 0x-prefixed 32-byte hex string');
      }
      return key as Hex;
    })(),
    maxGasLimitPerTx: BigInt(envOrDefault('MAX_GAS_LIMIT_PER_TX', '500000')),
    maxFeePerGasLimit: BigInt(envOrDefault('MAX_FEE_PER_GAS_LIMIT', '100000000000')),
    minSponsorBalance: BigInt(envOrDefault('MIN_SPONSOR_BALANCE', '1000000000000000000')),
    port: (() => {
      const p = Number(envOrDefault('PORT', '3000'));
      if (!Number.isInteger(p) || p <= 0) throw new Error('PORT must be a positive integer');
      return p;
    })(),
  };
}
