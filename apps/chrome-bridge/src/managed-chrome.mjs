import { spawn } from 'node:child_process';
import { existsSync, lstatSync, readlinkSync, unlinkSync } from 'node:fs';

const LOOPBACK = '127.0.0.1';

export function managedChromeCandidates(platform = process.platform) {
  if (platform === 'darwin') {
    return [
      '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
      '/Applications/Chromium.app/Contents/MacOS/Chromium',
    ];
  }
  if (platform === 'linux') {
    return [
      '/opt/google/chrome/chrome',
      '/usr/bin/google-chrome',
      '/usr/bin/google-chrome-stable',
      '/usr/bin/chromium',
      '/usr/bin/chromium-browser',
    ];
  }
  return [];
}

export function resolveManagedChromeExecutable(executable) {
  if (typeof executable === 'string' && executable.trim()) return executable;
  const configured = process.env.JAILGUN_CHROME || process.env.CHROME_PATH;
  if (configured) return configured;
  const detected = managedChromeCandidates().find((candidate) => existsSync(candidate));
  if (detected) return detected;
  throw new Error('managed-chrome-executable-missing');
}

export function managedChromeArgs({ profileDir, port, headless = false }) {
  if (!profileDir || !Number.isInteger(port) || port < 1024 || port > 65535) {
    throw new Error('managed-chrome-config-invalid');
  }
  return [
    `--user-data-dir=${profileDir}`,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-session-crashed-bubble',
    `--remote-debugging-port=${port}`,
    `--remote-debugging-address=${LOOPBACK}`,
    ...(headless ? ['--headless=new'] : []),
    '--new-window',
    'about:blank',
  ];
}

export function clearStaleProfileLocks(profileDir, processAlive = (pid) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error?.code === 'EPERM';
  }
}) {
  const lock = `${profileDir}/SingletonLock`;
  try {
    if (!lstatSync(lock).isSymbolicLink()) return false;
    const match = readlinkSync(lock).match(/-(\d+)$/);
    if (!match || processAlive(Number(match[1]))) return false;
    for (const name of ['SingletonLock', 'SingletonSocket', 'SingletonCookie']) {
      try { unlinkSync(`${profileDir}/${name}`); } catch (error) {
        if (error?.code !== 'ENOENT') throw error;
      }
    }
    return true;
  } catch (error) {
    if (error?.code === 'ENOENT') return false;
    throw error;
  }
}

export async function startManagedChrome({ executable, profileDir, port, headless = false, timeoutMs = 30_000 }) {
  const endpoint = `http://${LOOPBACK}:${port}`;
  try {
    const response = await fetch(`${endpoint}/json/version`, {
      cache: 'no-store',
      signal: AbortSignal.timeout(500),
    });
    if (response.ok) throw new Error('managed-chrome-port-in-use');
  } catch (error) {
    if (error?.message === 'managed-chrome-port-in-use') throw error;
  }
  clearStaleProfileLocks(profileDir);
  const child = spawn(resolveManagedChromeExecutable(executable), managedChromeArgs({ profileDir, port, headless }), {
    env: process.env,
    stdio: 'ignore',
  });
  let spawnError = null;
  child.once('error', (error) => { spawnError = error; });
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (spawnError) throw spawnError;
    if (child.exitCode !== null) throw new Error(`managed-chrome-exited:${child.exitCode}`);
    try {
      const response = await fetch(`${endpoint}/json/version`, {
        cache: 'no-store',
        signal: AbortSignal.timeout(1000),
      });
      const body = response.ok ? await response.json() : null;
      if (typeof body?.webSocketDebuggerUrl === 'string') return { child, endpoint };
    } catch {
      // Chrome has not opened its private loopback endpoint yet.
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  await stopManagedChrome(child);
  throw new Error('managed-chrome-start-timeout');
}

function running(child) {
  return child && child.exitCode === null && child.signalCode == null;
}

async function waitForChromeExit(child, timeoutMs) {
  if (!running(child)) return;
  await new Promise((resolve) => {
    const finish = () => { clearTimeout(timer); child.removeListener('exit', finish); resolve(); };
    const timer = setTimeout(finish, timeoutMs);
    child.once('exit', finish);
  });
}

export async function stopManagedChrome(child) {
  if (!running(child)) return;
  child.kill('SIGTERM');
  await waitForChromeExit(child, 2000);
  if (running(child)) {
    child.kill('SIGKILL');
    await waitForChromeExit(child, 2000);
  }
}

/** CDP disconnect alone does not close externally launched Chrome or flush its cookies. */
export async function closeManagedChrome(browser, child) {
  if (browser && running(child)) {
    try {
      const session = await browser.newBrowserCDPSession();
      await session.send('Browser.close');
      // The protocol response precedes disk flush and process exit. Do not signal
      // Chrome during this grace period: doing so loses newly authenticated cookies.
      await waitForChromeExit(child, 5000);
    } catch {
      // A lost CDP connection still requires bounded cleanup of our own process.
    }
  }
  await browser?.close().catch(() => {});
  await stopManagedChrome(child);
}
