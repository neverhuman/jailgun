import assert from 'node:assert/strict';
import { installBundle } from './install.mjs';

assert.equal(process.argv.length, 5, 'Usage: node install-bundle.mjs <bundle> <prefix> <verified-archive-sha256>');
console.log(JSON.stringify(await installBundle(process.argv[2], process.argv[3], process.argv[4])));
