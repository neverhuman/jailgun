#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { readFile, mkdir, mkdtemp, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { spawnSync } from 'node:child_process';

import { chromium } from 'playwright-core';

const args = parseArgs(process.argv.slice(2));
const url = required(args.url, '--url');
const cdpUrl = args.cdpUrl ?? 'http://127.0.0.1:9224';
const pollSeconds = Number(args.pollSeconds ?? 10);
const maxMinutes = Number(args.maxMinutes ?? 30);
const resultJson = args.resultJson ?? '';
const outputDir = args.outputDir ?? await mkdtemp(join(tmpdir(), 'jailgun-live-download-'));
const closeTab = args.close !== 'false';
const stopAfterDownload = args.stopAfterDownload !== 'false';

if (!Number.isFinite(pollSeconds) || pollSeconds < 1 || pollSeconds > 10) {
  throw new Error('--poll-seconds must be between 1 and 10');
}
if (!Number.isFinite(maxMinutes) || maxMinutes < 1) {
  throw new Error('--max-minutes must be positive');
}

await mkdir(outputDir, { recursive: true });

const browser = await chromium.connectOverCDP(cdpUrl, { timeout: 45_000 });
const context = browser.contexts()[0];
if (!context) {
  throw new Error(`No browser context found at ${cdpUrl}`);
}

const conversationId = conversationIdFromUrl(url);
let page = conversationId
  ? context.pages().find((candidate) => candidate.url().includes(conversationId))
  : null;
if (!page) {
  page = await context.newPage();
  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 60_000 });
}
await page.bringToFront();

let result = null;
const startedAt = Date.now();
console.log(`monitor:start url=${page.url()} poll_seconds=${pollSeconds} output_dir=${outputDir}`);

page.on('dialog', (dialog) => {
  const type = dialog.type();
  if (type === 'beforeunload') {
    console.log(`${new Date().toISOString()} dialog:beforeunload dismissed`);
    dialog.dismiss().catch(() => undefined);
  } else {
    console.log(`${new Date().toISOString()} dialog:${type} accepted message=${JSON.stringify(dialog.message().slice(0, 200))}`);
    dialog.accept().catch(() => undefined);
  }
});

try {
  while (Date.now() - startedAt < maxMinutes * 60_000) {
    const rateLimit = await dismissRateLimitModal(page);
    if (rateLimit.detected) {
      console.log(
        `${new Date().toISOString()} rate-limit:detected dismissed=${rateLimit.dismissed} button=${JSON.stringify(rateLimit.buttonLabel)} excerpt=${JSON.stringify(rateLimit.excerpt)}${rateLimit.reason ? ` reason=${JSON.stringify(rateLimit.reason)}` : ''}`
      );
    }
    const popups = await dismissPopups(page);
    for (const popup of popups) {
      console.log(
        `${new Date().toISOString()} popup:${popup.kind} clicked=${popup.clicked} button=${JSON.stringify(popup.buttonLabel)} excerpt=${JSON.stringify(popup.excerpt)}${popup.reason ? ` reason=${JSON.stringify(popup.reason)}` : ''}`
      );
    }
    const discovery = await discoverTarCandidates(page);
    const status = await readGenerationStatus(page);
    const ranked = rankCandidates(discovery.candidates);
    console.log(
      `${new Date().toISOString()} poll candidates=${ranked.length} mentions=${discovery.textMentions.length} active_stop=${status.activeStop} final_actions=${status.finalActions}`
    );

    if (ranked.length > 0) {
      const candidate = ranked[0];
      console.log(
        `${new Date().toISOString()} download:start index=${candidate.index} score=${candidate.score} label=${JSON.stringify(candidate.label)}`
      );
      const file = await downloadCandidate(page, candidate, outputDir);
      const stopResult = stopAfterDownload ? await stopIfGenerating(page) : { clicked: false, reason: 'disabled' };
      if (stopResult.clicked) {
        console.log(`${new Date().toISOString()} stop:clicked label=${JSON.stringify(stopResult.label)}`);
      } else {
        console.log(`${new Date().toISOString()} stop:not-needed reason=${stopResult.reason}`);
      }
      result = {
        status: 'downloaded',
        conversationUrl: page.url(),
        pollSeconds,
        outputDir,
        candidate,
        file,
        stopResult,
      };
      break;
    }

    if (!status.activeStop && status.finalActions > 0) {
      result = {
        status: 'done-no-tar',
        conversationUrl: page.url(),
        pollSeconds,
        outputDir,
        assistantRootCount: discovery.assistantRootCount,
        scannedControlCount: discovery.scannedControlCount,
        textMentions: discovery.textMentions,
      };
      break;
    }

    await page.waitForTimeout(pollSeconds * 1000);
  }

  if (!result) {
    result = {
      status: 'timeout-no-tar',
      conversationUrl: page.url(),
      pollSeconds,
      outputDir,
      maxMinutes,
    };
  }

  if (resultJson) {
    await writeFile(resultJson, JSON.stringify(result, null, 2));
  }
  console.log(`monitor:result ${JSON.stringify(result)}`);
} finally {
  if (closeTab) {
    await page.close().catch((error) => console.error(`tab close failed: ${error.message}`));
  }
  await browser.close().catch(() => undefined);
}

function parseArgs(argv) {
  const parsed = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith('--')) {
      continue;
    }
    const eq = arg.indexOf('=');
    if (eq >= 0) {
      parsed[toCamel(arg.slice(2, eq))] = arg.slice(eq + 1);
    } else {
      const key = toCamel(arg.slice(2));
      const next = argv[i + 1];
      if (next && !next.startsWith('--')) {
        parsed[key] = next;
        i += 1;
      } else {
        parsed[key] = 'true';
      }
    }
  }
  return parsed;
}

function toCamel(value) {
  return value.replace(/-([a-z])/g, (_, ch) => ch.toUpperCase());
}

function required(value, label) {
  if (!value || !String(value).trim()) {
    throw new Error(`${label} is required`);
  }
  return String(value);
}

function conversationIdFromUrl(value) {
  const match = String(value).match(/\/c\/([^/?#]+)/);
  return match?.[1] ?? '';
}

async function discoverTarCandidates(page) {
  return page.evaluate(() => {
    const controls = Array.from(document.querySelectorAll('a,button,[role="button"],[download],[href]'));
    const assistantRoots = Array.from(document.querySelectorAll('[data-message-author-role="assistant"]'));
    const textOf = (el) => String(el?.innerText || el?.textContent || '').replace(/\s+/g, ' ').trim();
    const attr = (el, name) => el?.getAttribute?.(name) || '';
    const href = (el) => el?.href || attr(el, 'href');
    const closestAssistant = (el) => el?.closest?.('[data-message-author-role="assistant"]') || null;
    const uploadChip = (el) => el?.closest?.('[data-testid*="upload-chip"]') || null;
    const visible = (el) => {
      const style = window.getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
    };
    const disabled = (el) => el.hasAttribute?.('disabled') || /^true$/i.test(attr(el, 'aria-disabled'));
    const tar = (value) => /\.tar(?:\(\d+\))?\.gz(?:$|[?#\s)])/i.test(String(value || ''));
    const candidates = [];
    for (let index = 0; index < controls.length; index += 1) {
      const el = controls[index];
      const assistant = closestAssistant(el);
      if ((assistantRoots.length > 0 && !assistant) || uploadChip(el)) {
        continue;
      }
      if (!visible(el) || disabled(el)) {
        continue;
      }
      const tag = String(el.tagName || '').toLowerCase();
      const role = attr(el, 'role').toLowerCase();
      const text = textOf(el);
      const entry = {
        index,
        text,
        href: href(el),
        download: attr(el, 'download'),
        aria: attr(el, 'aria-label'),
        title: attr(el, 'title'),
        tag,
        role,
        assistantIndex: assistant ? assistantRoots.indexOf(assistant) : null,
        score: 0,
      };
      const haystack = `${entry.text} ${entry.href} ${entry.download} ${entry.aria} ${entry.title}`;
      if (!tar(haystack)) {
        continue;
      }
      entry.label = entry.text || entry.download || entry.href || entry.aria || entry.title;
      entry.score += 200;
      if (/download/i.test(haystack)) entry.score += 100;
      if (tar(entry.download)) entry.score += 90;
      if (tar(entry.href)) entry.score += 80;
      if (tar(entry.text)) entry.score += 60;
      if (tag === 'button' || role === 'button') entry.score += 20;
      if (tag === 'a') entry.score += 10;
      if (assistant) entry.score += 30;
      candidates.push(entry);
    }
    const textMentions = [];
    const roots = assistantRoots.length > 0 ? assistantRoots : [document.body];
    for (const root of roots) {
      const matches = textOf(root).match(/[A-Za-z0-9._()-]+\.tar(?:\(\d+\))?\.gz/gi) || [];
      for (const name of matches) {
        textMentions.push(name);
      }
    }
    return {
      assistantRootCount: assistantRoots.length,
      scannedControlCount: controls.length,
      candidates,
      textMentions,
    };
  });
}

function rankCandidates(candidates) {
  return [...candidates]
    .filter((candidate) => !isDocumentTarLabelOnlyCandidate(candidate))
    .sort((a, b) => b.score - a.score);
}

function isDocumentTarLabelOnlyCandidate(candidate) {
  const href = String(candidate?.href || '');
  if (candidate?.assistantIndex != null) {
    return false;
  }
  if (tarNameLike(candidate?.download) || tarNameLike(href)) {
    return false;
  }
  return tarNameLike(`${candidate?.text || ''} ${candidate?.aria || ''} ${candidate?.title || ''} ${candidate?.label || ''}`);
}

function tarNameLike(value) {
  return /\.tar(?:\(\d+\))?\.gz(?:$|[?#\s)])/i.test(String(value || ''));
}

async function readGenerationStatus(page) {
  return page.evaluate(() => {
    const controls = Array.from(document.querySelectorAll('button,[role="button"],[aria-label],[title],[data-testid]'));
    const visible = (el) => {
      const style = window.getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
    };
    const disabled = (el) => el.hasAttribute?.('disabled') || /^true$/i.test(el.getAttribute?.('aria-disabled') || '');
    const label = (el) => [
      el.innerText || el.textContent || '',
      el.getAttribute?.('aria-label') || '',
      el.getAttribute?.('title') || '',
      el.getAttribute?.('data-testid') || '',
    ].join(' ').replace(/\s+/g, ' ').trim();
    let activeStop = false;
    let finalActions = 0;
    for (const el of controls) {
      if (!visible(el) || disabled(el)) {
        continue;
      }
      const text = label(el);
      if (/\b(stop answering|stop generating|stop responding|stop thinking|stop)\b/i.test(text)) {
        activeStop = true;
      }
      if (/\b(copy response|good response|bad response|more actions|sources)\b/i.test(text)) {
        finalActions += 1;
      }
    }
    return { activeStop, finalActions };
  });
}

async function downloadCandidate(page, candidate, outputDir) {
  const preFlight = await dismissRateLimitModal(page);
  if (preFlight.detected) {
    console.log(
      `${new Date().toISOString()} rate-limit:preflight dismissed=${preFlight.dismissed} button=${JSON.stringify(preFlight.buttonLabel)}${preFlight.reason ? ` reason=${JSON.stringify(preFlight.reason)}` : ''}`
    );
  }
  const downloadPromise = page.waitForEvent('download', { timeout: 120_000 });
  const locator = page.locator('a,button,[role="button"],[download],[href]').nth(candidate.index);
  await locator.scrollIntoViewIfNeeded({ timeout: 5_000 }).catch(() => undefined);
  await locator.click({ timeout: 120_000 });
  let download;
  try {
    download = await downloadPromise;
  } catch (error) {
    const recheck = await dismissRateLimitModal(page);
    if (recheck.detected) {
      console.log(
        `${new Date().toISOString()} rate-limit:on-timeout dismissed=${recheck.dismissed} button=${JSON.stringify(recheck.buttonLabel)} excerpt=${JSON.stringify(recheck.excerpt)}`
      );
      throw new Error(`download did not fire — rate-limit modal intercepted (button=${recheck.buttonLabel}, dismissed=${recheck.dismissed})`);
    }
    throw error;
  }
  const suggested = normalizeTarName(download.suggestedFilename() || basename(candidate.href || '') || 'chatgpt-output.tar.gz');
  const path = join(outputDir, suggested);
  await download.saveAs(path);
  const failure = await download.failure();
  if (failure) {
    throw new Error(`download failed: ${failure}`);
  }
  const fileStat = await stat(path);
  if (!fileStat.isFile() || fileStat.size === 0) {
    throw new Error(`downloaded file was empty or not a file: ${path}`);
  }
  const sha256 = await sha256File(path);
  const tarList = spawnSync('tar', ['-tzf', path], { encoding: 'utf8' });
  if (tarList.status !== 0) {
    throw new Error(`downloaded file is not a valid tar.gz: ${tarList.stderr.trim()}`);
  }
  const entries = tarList.stdout.trim().split('\n').filter(Boolean);
  if (entries.length === 0) {
    throw new Error(`downloaded tar.gz contained no entries: ${path}`);
  }
  return {
    path,
    suggested,
    sizeBytes: fileStat.size,
    sha256,
    tarEntryCount: entries.length,
    firstEntries: entries.slice(0, 10),
  };
}

async function dismissRateLimitModal(page) {
  try {
    return await page.evaluate(() => {
      const dialogSelector = '[role="dialog"],[aria-modal="true"]';
      const buttonSelector = 'button,[role="button"],a';
      const primary = /too many requests|making requests too quickly|temporarily limited access/i;
      const secondary = /please wait a few minutes|wait a few minutes before trying again/i;
      const buttonLabel = /^\s*got it\s*$/i;
      const visible = (el) => {
        const view = el.ownerDocument && el.ownerDocument.defaultView;
        if (!view) return true;
        const style = view.getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        return style.visibility !== 'hidden' && style.display !== 'none' && rect.width >= 0 && rect.height >= 0;
      };
      const disabled = (el) => el.hasAttribute('disabled') || /^true$/i.test(el.getAttribute('aria-disabled') || '');
      const textOf = (el) => String(el.textContent || '').replace(/\s+/g, ' ').trim();
      const dialogs = Array.from(document.querySelectorAll(dialogSelector));
      for (const dialog of dialogs) {
        if (!visible(dialog)) continue;
        const dialogText = textOf(dialog);
        if (!primary.test(dialogText) || !secondary.test(dialogText)) continue;
        const buttons = Array.from(dialog.querySelectorAll(buttonSelector));
        for (const button of buttons) {
          if (!visible(button) || disabled(button)) continue;
          const label = textOf(button) || button.getAttribute('aria-label') || button.getAttribute('title') || '';
          if (!buttonLabel.test(label)) continue;
          try {
            button.click();
          } catch (error) {
            return {
              detected: true,
              dismissed: false,
              excerpt: dialogText.slice(0, 240),
              buttonLabel: label,
              reason: `click-failed: ${error.message}`,
            };
          }
          return {
            detected: true,
            dismissed: true,
            excerpt: dialogText.slice(0, 240),
            buttonLabel: label,
          };
        }
        return {
          detected: true,
          dismissed: false,
          excerpt: dialogText.slice(0, 240),
          buttonLabel: '',
          reason: 'no-got-it-button',
        };
      }
      return { detected: false, dismissed: false, excerpt: '', buttonLabel: '' };
    });
  } catch (error) {
    return {
      detected: false,
      dismissed: false,
      excerpt: '',
      buttonLabel: '',
      reason: `evaluate-failed: ${error.message}`,
    };
  }
}

async function dismissPopups(page) {
  try {
    return await page.evaluate(() => {
      const dialogSelector = '[role="dialog"],[aria-modal="true"]';
      const buttonSelector = 'button,[role="button"],a';
      const stayPrimary = /leave (this )?(page|site)|reload (this )?(page|site)/i;
      const staySecondary = /changes (you'?ve |you have )?made|might not be saved|won'?t be saved|aren'?t saved|are not saved|unsaved/i;
      const stayButtonLabel = /^\s*stay( on (this )?page)?\s*$/i;
      const sessionExpired = /session (has )?expired|you'?ve been signed out|you have been signed out|please (sign|log) (back )?in/i;
      const visible = (el) => {
        const view = el.ownerDocument && el.ownerDocument.defaultView;
        if (!view) return true;
        const style = view.getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        return style.visibility !== 'hidden' && style.display !== 'none' && rect.width >= 0 && rect.height >= 0;
      };
      const disabled = (el) => el.hasAttribute('disabled') || /^true$/i.test(el.getAttribute('aria-disabled') || '');
      const textOf = (el) => String(el.textContent || '').replace(/\s+/g, ' ').trim();
      const labelOf = (el) => textOf(el) || el.getAttribute('aria-label') || el.getAttribute('title') || '';

      const outcomes = [];
      const dialogs = Array.from(document.querySelectorAll(dialogSelector));
      for (const dialog of dialogs) {
        if (!visible(dialog)) continue;
        const dialogText = textOf(dialog);

        if (stayPrimary.test(dialogText) && staySecondary.test(dialogText)) {
          const buttons = Array.from(dialog.querySelectorAll(buttonSelector));
          let clicked = false;
          let buttonLabel = '';
          let reason;
          for (const button of buttons) {
            if (!visible(button) || disabled(button)) continue;
            const bl = labelOf(button);
            if (!stayButtonLabel.test(bl)) continue;
            buttonLabel = bl;
            try {
              button.click();
              clicked = true;
            } catch (error) {
              reason = `click-failed: ${error.message}`;
            }
            break;
          }
          outcomes.push({
            kind: 'stay-on-page',
            buttonLabel,
            excerpt: dialogText.slice(0, 240),
            clicked,
            reason: clicked ? reason : (reason || 'no-stay-button'),
          });
          continue;
        }

        if (sessionExpired.test(dialogText)) {
          outcomes.push({
            kind: 'session-expired',
            buttonLabel: '',
            excerpt: dialogText.slice(0, 240),
            clicked: false,
            reason: 'detect-only',
          });
          continue;
        }
      }
      return outcomes;
    });
  } catch (error) {
    console.log(`${new Date().toISOString()} popup:evaluate-failed reason=${JSON.stringify(error.message)}`);
    return [];
  }
}

async function stopIfGenerating(page) {
  return page.evaluate(() => {
    const controls = Array.from(document.querySelectorAll('button,[role="button"],[aria-label],[title]'));
    const visible = (el) => {
      const style = window.getComputedStyle(el);
      const rect = el.getBoundingClientRect();
      return style.visibility !== 'hidden' && style.display !== 'none' && rect.width > 0 && rect.height > 0;
    };
    const disabled = (el) => el.hasAttribute?.('disabled') || /^true$/i.test(el.getAttribute?.('aria-disabled') || '');
    const label = (el) => [
      el.innerText || el.textContent || '',
      el.getAttribute?.('aria-label') || '',
      el.getAttribute?.('title') || '',
    ].join(' ').replace(/\s+/g, ' ').trim();
    for (const el of controls) {
      if (!visible(el) || disabled(el)) {
        continue;
      }
      const text = label(el);
      if (/\b(stop answering|stop generating|stop responding|stop thinking|stop)\b/i.test(text)) {
        el.click();
        return { clicked: true, label: text };
      }
    }
    return { clicked: false, reason: 'not-found' };
  });
}

function normalizeTarName(value) {
  const safe = String(value || 'chatgpt-output.tar.gz').replace(/[/\\]/g, '-');
  const normalized = safe
    .replace(/\.tar\(\d+\)\.gz$/i, '.tar.gz')
    .replace(/\.tgz$/i, '.tar.gz')
    .replace(/\.gz\.tar\.gz$/i, '.gz');
  return /\.tar\.gz$/i.test(normalized) ? normalized : `${normalized}.tar.gz`;
}

async function sha256File(path) {
  return createHash('sha256').update(await readFile(path)).digest('hex');
}
