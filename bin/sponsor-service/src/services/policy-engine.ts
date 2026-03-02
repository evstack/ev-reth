import type { SponsorableIntent } from '@evstack/evnode-viem';
import { validateEvNodeTx, recoverExecutor } from '@evstack/evnode-viem';
import type { SponsorConfig } from '../config.js';
import {
  INVALID_INTENT,
  INVALID_EXECUTOR_SIGNATURE,
  CHAIN_ID_MISMATCH,
  GAS_LIMIT_EXCEEDED,
  FEE_TOO_HIGH,
} from '../errors.js';

export class PolicyEngine {
  constructor(private config: SponsorConfig) {}

  async validate(intent: SponsorableIntent): Promise<void> {
    try {
      validateEvNodeTx(intent.tx);
    } catch (e) {
      throw INVALID_INTENT((e as Error).message);
    }

    if (intent.tx.chainId !== this.config.chainId) {
      throw CHAIN_ID_MISMATCH(this.config.chainId, intent.tx.chainId);
    }

    const recoveredExecutor = await recoverExecutor({
      transaction: intent.tx,
      executorSignature: intent.executorSignature,
    });
    if (recoveredExecutor.toLowerCase() !== intent.executorAddress.toLowerCase()) {
      throw INVALID_EXECUTOR_SIGNATURE();
    }

    if (intent.tx.gasLimit > this.config.maxGasLimitPerTx) {
      throw GAS_LIMIT_EXCEEDED(this.config.maxGasLimitPerTx, intent.tx.gasLimit);
    }

    if (intent.tx.maxFeePerGas > this.config.maxFeePerGasLimit) {
      throw FEE_TOO_HIGH(this.config.maxFeePerGasLimit, intent.tx.maxFeePerGas);
    }
  }
}
