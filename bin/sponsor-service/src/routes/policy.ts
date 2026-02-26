import type { FastifyInstance } from 'fastify';
import type { Address } from 'viem';
import type { SponsorConfig } from '../config.js';

export function registerPolicyRoute(app: FastifyInstance, config: SponsorConfig, sponsorAddress: Address) {
  app.get('/v1/policy', async (_request, reply) => {
    return reply.send({
      chainId: config.chainId.toString(),
      sponsorAddress,
      maxGasPerTx: config.maxGasLimitPerTx.toString(),
      maxFeePerGas: config.maxFeePerGasLimit.toString(),
    });
  });
}
