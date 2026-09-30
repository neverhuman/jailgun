import { createHash } from 'node:crypto';
import { copyFile, mkdir, readdir, lstat, readFile, writeFile, chmod } from 'node:fs/promises';
import { dirname, join } from 'node:path';

export const hash = bytes => createHash('sha256').update(bytes).digest('hex');
export const fileHash = async path => hash(await readFile(path));
export async function json(path, value) { await mkdir(dirname(path), { recursive: true });await writeFile(path, `${JSON.stringify(value, null, 2)}\n`); }
export async function copy(source, destination) {
  const stat = await lstat(source);
  if (stat.isDirectory()) {
    await mkdir(destination, { recursive: true });
    for (const name of await readdir(source)) await copy(join(source, name), join(destination, name));
  } else if (stat.isFile()) {
    await mkdir(dirname(destination), { recursive: true });await copyFile(source, destination);
    await chmod(destination, stat.mode & 0o111 ? 0o755 : 0o644);
  } else throw new Error(`Distribution input is not a regular file or directory: ${source}`);
}
export async function files(root, prefix = '') {
  const result = [];
  for (const name of (await readdir(join(root, prefix))).sort()) {
    const path = prefix ? `${prefix}/${name}` : name;
    if (/[\x00-\x1f\\]/.test(path)) throw new Error('Unsafe distribution file name');
    const stat = await lstat(join(root, path));
    if (stat.isDirectory()) result.push(...await files(root, path));
    else if (stat.isFile()) result.push({ path, bytes: stat.size, executable: Boolean(stat.mode & 0o111), sha256: await fileHash(join(root, path)) });
    else throw new Error(`Distribution contains a link or special file: ${path}`);
  }
  return result;
}
