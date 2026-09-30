import { createServer as createHttpServer } from 'node:http';

import { createStateRegistry } from './state.mjs';
import { makeRouteHandler, DEFAULTS } from './routes.mjs';
import { createConceptFixture } from './concept.mjs';

export function createServer({ fixturesDir = DEFAULTS.DEFAULT_FIXTURES_DIR, interactive = false } = {}) {
  const registry = createStateRegistry();
  const concept = interactive ? createConceptFixture() : null;
  const handler = concept ? concept.handle : makeRouteHandler({ registry, fixturesDir });
  const server = createHttpServer((req, res) => {
    handler(req, res).catch((error) => {
      res.statusCode = 500;
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify({ error: error.message }));
    });
  });
  return { server, registry, concept };
}

export async function start({ port = 8082, fixturesDir, interactive = false } = {}) {
  const { server, registry, concept } = createServer({ fixturesDir, interactive });
  await new Promise((resolve) => server.listen(port, '127.0.0.1', resolve));
  const address = server.address();
  const boundPort = typeof address === 'object' && address ? address.port : port;
  return {
    server,
    registry,
    concept,
    port: boundPort,
    url: `http://127.0.0.1:${boundPort}`,
    async stop() {
      await new Promise((resolve, reject) => {
        server.close((error) => (error ? reject(error) : resolve()));
      });
    },
  };
}
