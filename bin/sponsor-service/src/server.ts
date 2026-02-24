import Fastify from 'fastify';
import type { SponsorConfig } from './config.js';
import { SponsorService } from './services/sponsor-service.js';
import { registerJsonRpcRoute } from './routes/jsonrpc.js';
import { registerSponsorRoute } from './routes/sponsor.js';
import { registerPolicyRoute } from './routes/policy.js';
import { registerHealthRoute } from './routes/health.js';

export function createServer(config: SponsorConfig) {
  const app = Fastify({ logger: true });
  const service = new SponsorService(config);

  registerJsonRpcRoute(app, service);
  registerSponsorRoute(app, service);
  registerPolicyRoute(app, config, service.sponsorAddress);
  registerHealthRoute(app, service);

  return { app, service };
}
