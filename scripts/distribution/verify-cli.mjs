import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import { verifyBundle } from './verify.mjs';

assert.equal(process.argv.length, 3, 'Usage: node verify-bundle.mjs <bundle-directory>');
console.log(JSON.stringify(await verifyBundle(resolve(process.argv[2]))));
