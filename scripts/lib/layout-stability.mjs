import assert from 'node:assert/strict';

export async function observeLayoutStability(page) {
  await page.addInitScript(() => {
    const entries = [];
    let overflow = false;
    const supported = PerformanceObserver.supportedEntryTypes.includes('layout-shift');
    const append = records => {
      for (const record of records) {
        if (entries.length >= 500) { overflow = true;continue; }
        entries.push({ value: record.value, startTime: record.startTime, hadRecentInput: record.hadRecentInput,
          sources: record.sources.map(source => source.node?.id || source.node?.className || source.node?.tagName || 'unavailable') });
      }
    };
    const observer = new PerformanceObserver(list => append(list.getEntries()));
    if (supported) observer.observe({ type: 'layout-shift', buffered: true });
    window.jailgunLayoutStability = () => { append(observer.takeRecords());return { supported, overflow, entries }; };
  });
}

// CLS is the maximum session window: at most five seconds, gaps below one second,
// excluding shifts within the browser's recent-input interval.
export function cumulativeLayoutShift(entries) {
  let first = -Infinity;
  let last = -Infinity;
  let windowValue = 0;
  let maximum = 0;
  for (const entry of entries) {
    if (entry.hadRecentInput) continue;
    if (entry.startTime - last >= 1000 || entry.startTime - first >= 5000) {
      first = entry.startTime;windowValue = 0;
    }
    last = entry.startTime;
    windowValue += entry.value;
    maximum = Math.max(maximum, windowValue);
  }
  return maximum;
}

export async function measureLayoutStability(page) {
  await page.evaluate(async () => {
    await document.fonts.ready;
    await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
  });
  const observed = await page.evaluate(() => window.jailgunLayoutStability());
  assert.equal(observed.supported, true, 'browser does not expose layout-shift measurements');
  assert.equal(observed.overflow, false, 'layout-shift evidence exceeded its collection limit');
  return { cls: cumulativeLayoutShift(observed.entries), ...observed };
}

export async function verifyLayoutInstrumentation(context) {
  assert.equal(cumulativeLayoutShift([{ value: .7, startTime: 1, hadRecentInput: true }, { value: .04, startTime: 1000 }, { value: .03, startTime: 1500 }, { value: .06, startTime: 3000 }]), .07);
  assert.equal(cumulativeLayoutShift(Array.from({ length: 7 }, (_, i) => ({ value: 1, startTime: i * 900 }))), 6);
  const page = await context.newPage();
  try {
    await page.setViewportSize({ width: 800, height: 600 });
    await observeLayoutStability(page);
    await page.goto('data:text/html,<style>body{margin:0}</style><div id="space"></div><div style="height:300px;background:green">Synthetic measurement control</div>');
    const before = await measureLayoutStability(page);
    await page.locator('#space').evaluate(element => { element.style.height = '300px'; });
    const after = await measureLayoutStability(page);
    assert.equal(before.cls, 0);
    assert.ok(after.cls > .1, 'measurement did not detect an intentionally displaced visible element');
    return { status: 'pass', before: before.cls, after: after.cls, entries: after.entries };
  } finally { await page.close(); }
}
