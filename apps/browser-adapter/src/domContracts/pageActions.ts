export async function closeTabAfterReceipt(
  page: { close: (options?: unknown) => Promise<void>; locator: (selector: string) => { click?: () => Promise<void>; count?: () => Promise<number> } },
  receiptConfirmed: boolean
): Promise<boolean> {
  if (!receiptConfirmed) {
    return false;
  }
  const stop = page.locator('button[aria-label*="Stop"],button:has-text("Stop"),[role="button"]:has-text("Stop")');
  const stopCount = await stop.count?.().catch(() => 0);
  if ((stopCount ?? 0) > 0) {
    await stop.click?.().catch(() => undefined);
  }
  await page.close({ runBeforeUnload: false });
  return true;
}
