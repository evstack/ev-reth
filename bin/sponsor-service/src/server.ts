import Fastify from 'fastify';
import type { SponsorConfig } from './config.js';
import { SponsorService } from './services/sponsor-service.js';
import { registerJsonRpcRoute } from './routes/jsonrpc.js';
import { registerPolicyRoute } from './routes/policy.js';
import { registerHealthRoute } from './routes/health.js';

export function createServer(config: SponsorConfig) {
  const app = Fastify({ logger: true });
  const service = new SponsorService(config);

  registerJsonRpcRoute(app, service);
  registerPolicyRoute(app, config, service.sponsorAddress);
  registerHealthRoute(app, service);

  return { app, service };
}
