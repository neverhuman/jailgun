#!/usr/bin/env node
import { start } from '../src/server.mjs';

function parseArgs(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith('--')) continue;
    const eq = arg.indexOf('=');
    if (eq >= 0) {
      out[arg.slice(2, eq)] = arg.slice(eq + 1);
    } else {
      const next = argv[i + 1];
      if (next && !next.startsWith('--')) {
        out[arg.slice(2)] = next;
        i += 1;
      } else {
        out[arg.slice(2)] = 'true';
      }
    }
  }
  return out;
}

const args = parseArgs(process.argv.slice(2));
const port = Number(args.port ?? 8082);
const fixturesDir = args['fixtures-dir'];

const handle = await start({ port, fixturesDir });
console.log(`fake-chatgpt listening on ${handle.url}`);
console.log('admin endpoints: POST /admin/state, /admin/advance, /admin/reset; GET /admin/status');

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, async () => {
    console.log(`received ${signal}, shutting down`);
    try {
      await handle.stop();
    } catch (error) {
      console.error(`shutdown error: ${error.message}`);
    }
    process.exit(0);
  });
}
