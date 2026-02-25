import type { Address, Hex } from 'viem';
import { hexToSignature } from 'viem';
import { privateKeyToAccount } from 'viem/accounts';
import {
  signAsSponsor,
  encodeSignedTransaction,
  type SponsorableIntent,
  type HashSigner,
  type EvNodeSignedTransaction,
} from '@evstack/evnode-viem';
import type { SponsorConfig } from '../config.js';
import { PolicyEngine } from './policy-engine.js';
import { RpcClient } from './rpc-client.js';
import { SPONSOR_BALANCE_LOW } from '../errors.js';

export interface SponsorResult {
  txHash: Hex;
  sponsorAddress: Address;
}

export class SponsorService {
  public readonly policyEngine: PolicyEngine;
  public readonly rpc: RpcClient;
  private readonly sponsorSigner: HashSigner;
  private readonly minBalance: bigint;

  constructor(config: SponsorConfig, rpc?: RpcClient) {
    this.policyEngine = new PolicyEngine(config);
    this.rpc = rpc ?? new RpcClient(config.rpcUrl);
    this.minBalance = config.minSponsorBalance;

    const account = privateKeyToAccount(config.sponsorPrivateKey);
    this.sponsorSigner = {
      address: account.address,
      signHash: async (hash: Hex) => {
        const sig = await account.sign({ hash });
        return hexToSignature(sig);
      },
    };
  }

  get sponsorAddress(): Address {
    return this.sponsorSigner.address;
  }

  async sponsorIntent(intent: SponsorableIntent): Promise<SponsorResult> {
    await this.policyEngine.validate(intent);

    const balance = await this.rpc.getBalance(this.sponsorSigner.address);
    if (balance < this.minBalance) {
      throw SPONSOR_BALANCE_LOW();
    }

    const feePayerSignature = await signAsSponsor(
      intent.tx,
      intent.executorAddress,
      this.sponsorSigner,
    );

    const signedTx: EvNodeSignedTransaction = {
      transaction: { ...intent.tx, feePayerSignature },
      executorSignature: intent.executorSignature,
    };
    const encoded = encodeSignedTransaction(signedTx);
    const txHash = await this.rpc.sendRawTransaction(encoded);

    return { txHash, sponsorAddress: this.sponsorSigner.address };
  }
}
