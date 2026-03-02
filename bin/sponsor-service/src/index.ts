import { loadConfig } from './config.js';
import { createServer } from './server.js';

const config = loadConfig();
const { app } = createServer(config);

app.listen({ port: config.port, host: '0.0.0.0' }, (err, address) => {
  if (err) {
    app.log.error(err);
    process.exit(1);
  }
  app.log.info(`Sponsor service listening on ${address}`);
});
