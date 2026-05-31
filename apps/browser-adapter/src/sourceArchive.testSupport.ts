import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';

type FixtureFile = {
  path: string;
  contents: string | Buffer;
};

export async function withGitFixture<T>(files: FixtureFile[], run: (fixtureRoot: string) => Promise<T>): Promise<T> {
  const fixtureRoot = await mkdtemp(join(tmpdir(), 'jailgun-git-fixture-'));
  try {
    runGit(fixtureRoot, ['init']);
    runGit(fixtureRoot, ['config', 'user.email', 'test@example.invalid']);
    runGit(fixtureRoot, ['config', 'user.name', 'Test User']);

    for (const file of files) {
      const parent = dirname(file.path);
      if (parent !== '.') {
        await mkdir(join(fixtureRoot, parent), { recursive: true });
      }
      await writeFile(join(fixtureRoot, file.path), file.contents);
    }

    runGit(fixtureRoot, ['add', '.']);
    runGit(fixtureRoot, ['commit', '-m', 'fixture']);
    return await run(fixtureRoot);
  } finally {
    await rm(fixtureRoot, { recursive: true, force: true });
  }
}

export function archiveEntries(archivePath: string): string[] {
  const result = spawnSync('tar', ['-tzf', archivePath], { encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(result.stderr);
  }
  return result.stdout
    .trim()
    .split('\n')
    .filter((entry) => entry !== 'pax_global_header')
    .sort();
}

function runGit(cwd: string, args: string[]): void {
  const result = spawnSync('git', args, { cwd, encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(`git ${args.join(' ')} exited ${result.status}: ${result.stderr.trim()}`);
  }
}
