import { createServer as createHttpServer } from 'node:http';

import { createStateRegistry } from './state.mjs';
import { makeRouteHandler, DEFAULTS } from './routes.mjs';

export function createServer({ fixturesDir = DEFAULTS.DEFAULT_FIXTURES_DIR } = {}) {
  const registry = createStateRegistry();
  const handler = makeRouteHandler({ registry, fixturesDir });
  const server = createHttpServer((req, res) => {
    handler(req, res).catch((error) => {
      res.statusCode = 500;
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify({ error: error.message }));
    });
  });
  return { server, registry };
}

export async function start({ port = 8082, fixturesDir } = {}) {
  const { server, registry } = createServer({ fixturesDir });
  await new Promise((resolve) => server.listen(port, '127.0.0.1', resolve));
  const address = server.address();
  const boundPort = typeof address === 'object' && address ? address.port : port;
  return {
    server,
    registry,
    port: boundPort,
    url: `http://127.0.0.1:${boundPort}`,
    async stop() {
      await new Promise((resolve, reject) => {
        server.close((error) => (error ? reject(error) : resolve()));
      });
    },
  };
}
