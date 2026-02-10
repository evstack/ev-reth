import type { AccessList, Address, Client, Hex, Signature } from 'viem';

export const EVNODE_TX_TYPE = 0x76;
export const EVNODE_EXECUTOR_DOMAIN = 0x76;
export const EVNODE_SPONSOR_DOMAIN = 0x78;

export type RlpValue = Hex | RlpValue[];

export interface Call {
  to: Address | null;
  value: bigint;
  data: Hex;
}

export interface EvNodeTransaction {
  chainId: bigint;
  nonce: bigint;
  maxPriorityFeePerGas: bigint;
  maxFeePerGas: bigint;
  gasLimit: bigint;
  calls: Call[];
  accessList: AccessList;
  feePayerSignature?: Signature;
}

export interface EvNodeSignedTransaction {
  transaction: EvNodeTransaction;
  executorSignature: Signature;
}

export interface SponsorableIntent {
  tx: EvNodeTransaction;
  executorSignature: Signature;
  executorAddress: Address;
}

export interface HashSigner {
  address: Address;
  // Must sign the raw 32-byte hash without EIP-191 prefixing.
  signHash: (hash: Hex) => Promise<Signature>;
}

export interface EvnodeClientOptions {
  client: Client;
  executor?: HashSigner;
  sponsor?: HashSigner;
}

export interface EvnodeSendArgs {
  calls: Call[];
  executor?: HashSigner;
  chainId?: bigint;
  nonce?: bigint;
  maxFeePerGas?: bigint;
  maxPriorityFeePerGas?: bigint;
  gasLimit?: bigint;
  accessList?: AccessList;
}

export type EvnodeSendArgsWithExecutor = Omit<EvnodeSendArgs, 'executor'> & {
  executor: HashSigner;
};

export interface EvnodeSponsorArgs {
  intent: SponsorableIntent;
  sponsor?: HashSigner;
}
