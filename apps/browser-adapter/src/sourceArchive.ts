import { createWriteStream } from 'node:fs';
import { mkdtemp, mkdir, rm, stat } from 'node:fs/promises';
import { basename, extname, isAbsolute, join } from 'node:path';
import { spawn } from 'node:child_process';

export const DEFAULT_SOURCE_ARCHIVE_TMP_PARENT = '/tmp';
export const DEFAULT_SOURCE_ARCHIVE_MODE = 'ai-source';

export type SourceArchiveMode = 'ai-source' | 'full';

export interface SourceArchiveOptions {
  repoUrl: string;
  refName?: string;
  prefix: string;
  archiveFilename: string;
  tmpParent?: string;
  mode?: SourceArchiveMode;
}

export interface SourceArchiveResult {
  tempRoot: string;
  cloneDir: string;
  archivePath: string;
  archiveFilename: string;
  commit: string;
}

export async function createTempSourceArchive(options: SourceArchiveOptions): Promise<SourceArchiveResult> {
  validateArchiveOptions(options);
  const tmpParent = options.tmpParent ?? DEFAULT_SOURCE_ARCHIVE_TMP_PARENT;
  await mkdir(tmpParent, { recursive: true });
  const tempRoot = await mkdtemp(join(tmpParent, 'jailgun-source-'));
  const cloneDir = join(tempRoot, 'repo');
  const archivePath = join(tempRoot, basename(options.archiveFilename));

  try {
    await runGit(['clone', '--depth=1', options.repoUrl, cloneDir]);
    if (options.refName && options.refName !== 'HEAD') {
      await runGit(['fetch', '--depth=1', 'origin', options.refName], cloneDir);
      await runGit(['checkout', 'FETCH_HEAD'], cloneDir);
    }
    const commit = (await runGit(['rev-parse', 'HEAD'], cloneDir)).trim();
    const mode = options.mode ?? DEFAULT_SOURCE_ARCHIVE_MODE;
    const selectedPaths = mode === 'ai-source' ? await listAiSourcePaths(cloneDir) : null;
    await gitArchive(cloneDir, options.prefix, archivePath, selectedPaths);
    const archiveStat = await stat(archivePath);
    if (!archiveStat.isFile() || archiveStat.size === 0) {
      throw new Error(`archive was not created: ${archivePath}`);
    }
    return {
      tempRoot,
      cloneDir,
      archivePath,
      archiveFilename: basename(archivePath),
      commit
    };
  } catch (error) {
    await cleanupSourceArchive({ tempRoot }).catch(() => undefined);
    throw error;
  }
}

export async function cleanupSourceArchive(result: Pick<SourceArchiveResult, 'tempRoot'>): Promise<void> {
  await rm(result.tempRoot, { recursive: true, force: true });
}

function validateArchiveOptions(options: SourceArchiveOptions): void {
  if (!options.repoUrl.trim()) {
    throw new Error('repoUrl is required');
  }
  if (options.tmpParent && !isAbsolute(options.tmpParent)) {
    throw new Error('tmpParent must be an absolute path');
  }
  if (!options.prefix.endsWith('/') || options.prefix.startsWith('/') || options.prefix.includes('..')) {
    throw new Error('prefix must be a relative directory ending with /');
  }
  if (!options.archiveFilename.endsWith('.tar.gz')) {
    throw new Error('archiveFilename must end with .tar.gz');
  }
  if (basename(options.archiveFilename) !== options.archiveFilename || options.archiveFilename.includes('..')) {
    throw new Error('archiveFilename must be a safe basename');
  }
  if (options.mode && options.mode !== 'ai-source' && options.mode !== 'full') {
    throw new Error('mode must be ai-source or full');
  }
}

async function listAiSourcePaths(cloneDir: string): Promise<string[]> {
  const output = await runGit(['ls-tree', '-r', '--name-only', '-z', 'HEAD'], cloneDir);
  const paths = output
    .split('\0')
    .filter((path) => path.length > 0)
    .filter(isAiSourcePath);
  if (paths.length === 0) {
    throw new Error('source archive filter produced no useful code or Markdown files');
  }
  return paths;
}

function isAiSourcePath(path: string): boolean {
  const parts = path.split('/').filter(Boolean);
  if (parts.length === 0) {
    return false;
  }
  if (parts.some((part) => EXCLUDED_DIRECTORIES.has(part.toLowerCase()))) {
    return false;
  }

  const filename = parts[parts.length - 1];
  const lowerFilename = filename.toLowerCase();
  if (EXCLUDED_FILENAMES.has(lowerFilename)) {
    return false;
  }

  const extension = extname(lowerFilename);
  return MARKDOWN_EXTENSIONS.has(extension) || CODE_EXTENSIONS.has(extension) || CODE_FILENAMES.has(lowerFilename);
}

const MARKDOWN_EXTENSIONS = new Set(['.md', '.mdx']);

const CODE_EXTENSIONS = new Set([
  '.bash',
  '.c',
  '.cc',
  '.cjs',
  '.cpp',
  '.cs',
  '.css',
  '.fish',
  '.go',
  '.graphql',
  '.h',
  '.hh',
  '.hpp',
  '.html',
  '.java',
  '.js',
  '.jsx',
  '.kt',
  '.kts',
  '.lua',
  '.mjs',
  '.nix',
  '.php',
  '.proto',
  '.py',
  '.rb',
  '.rs',
  '.scss',
  '.sh',
  '.sql',
  '.swift',
  '.tf',
  '.toml',
  '.ts',
  '.tsx',
  '.vim',
  '.yaml',
  '.yml',
  '.zsh'
]);

const CODE_FILENAMES = new Set([
  '.dockerignore',
  '.editorconfig',
  '.gitattributes',
  '.gitignore',
  'dockerfile',
  'justfile',
  'makefile',
  'package.json',
  'pyproject.toml',
  'requirements.in',
  'requirements.txt',
  'go.mod',
  'go.sum',
  'cargo.toml'
]);

const EXCLUDED_FILENAMES = new Set([
  'cargo.lock',
  'package-lock.json',
  'pnpm-lock.yaml',
  'poetry.lock',
  'yarn.lock'
]);

const EXCLUDED_DIRECTORIES = new Set([
  '.cache',
  '.git',
  '.next',
  '.nuxt',
  '.parcel-cache',
  '.svelte-kit',
  '.turbo',
  '.venv',
  'artifacts',
  'build',
  'coverage',
  'dist',
  'downloads',
  'logs',
  'node_modules',
  'out',
  'target',
  'tmp',
  'vendor'
]);

async function gitArchive(
  cloneDir: string,
  prefix: string,
  archivePath: string,
  selectedPaths: string[] | null
): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    const args = ['archive', '--format=tar.gz', `--prefix=${prefix}`, 'HEAD'];
    if (selectedPaths) {
      args.push('--', ...selectedPaths);
    }
    const child = spawn('git', args, {
      cwd: cloneDir,
      stdio: ['ignore', 'pipe', 'pipe']
    });
    const output = createWriteStream(archivePath);
    let stderr = '';
    let childClosed = false;
    let outputClosed = false;
    let childCode: number | null = null;
    let settled = false;

    const fail = (error: Error) => {
      if (settled) {
        return;
      }
      settled = true;
      child.kill();
      output.destroy();
      reject(error);
    };
    const maybeResolve = () => {
      if (settled || !childClosed || !outputClosed) {
        return;
      }
      settled = true;
      if (childCode === 0) {
        resolve();
      } else {
        reject(new Error(`git archive exited ${childCode}: ${stderr.trim()}`));
      }
    };

    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk: string) => {
      stderr += chunk;
    });
    child.once('error', fail);
    child.once('close', (code) => {
      childClosed = true;
      childCode = code;
      maybeResolve();
    });
    output.once('error', fail);
    output.once('close', () => {
      outputClosed = true;
      maybeResolve();
    });
    child.stdout.pipe(output);
  });
}

async function runGit(args: string[], cwd?: string): Promise<string> {
  return new Promise<string>((resolve, reject) => {
    const child = spawn('git', args, {
      cwd,
      stdio: ['ignore', 'pipe', 'pipe']
    });
    let stdout = '';
    let stderr = '';
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk: string) => {
      stdout += chunk;
    });
    child.stderr.on('data', (chunk: string) => {
      stderr += chunk;
    });
    child.once('error', reject);
    child.once('close', (code) => {
      if (code === 0) {
        resolve(stdout);
      } else {
        reject(new Error(`git ${args.join(' ')} exited ${code}: ${stderr.trim()}`));
      }
    });
  });
}
