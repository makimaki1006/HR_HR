import type { Page } from '@playwright/test';

/** 条件の操作欄を固定して開く (既に開いていればそのまま) */
export async function openQueueToolbar(page: Page) {
  const toggle = page.getByRole('button', { name: /^条件を変更:/ });
  if (await toggle.getAttribute('aria-pressed') !== 'true') await toggle.click();
}
