import type { FastifyInstance } from 'fastify';
import type { SponsorService } from '../services/sponsor-service.js';
import { deserializeIntent, type SerializedSponsorableIntent } from '../serialization.js';
import { SponsorError } from '../errors.js';

export function registerSponsorRoute(app: FastifyInstance, service: SponsorService) {
  app.post('/v1/sponsor', async (request, reply) => {
    try {
      const intent = deserializeIntent(request.body as SerializedSponsorableIntent);
      const result = await service.sponsorIntent(intent);
      return reply.send(result);
    } catch (e) {
      if (e instanceof SponsorError) {
        return reply.status(e.statusCode).send({ error: e.code, message: e.message });
      }
      throw e;
    }
  });
}
