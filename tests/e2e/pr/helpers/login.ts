import { expect, Page } from '@playwright/test';
import { E2E_EMAIL, E2E_PASSWORD, PR_BASE_URL } from './fixture_values';

/** ログインして / のダッシュボード (ヘッダーの「ログイン: <email>」) が出るまで待つ。 */
export async function login(page: Page): Promise<void> {
  await page.goto('/login');
  await page.fill('#email', E2E_EMAIL);
  await page.fill('#password', E2E_PASSWORD);
  await Promise.all([
    page.waitForURL((u) => u.pathname === '/'),
    page.click("form[action='/login'] button[type=submit]"),
  ]);
  await expect(page.getByText(`ログイン: ${E2E_EMAIL}`)).toBeVisible();
}

/**
 * 認証後の POST。サーバは debug ビルドで CSRF_EXTRA_ORIGINS_DEBUG=PR_BASE_URL 付きで起動しているので
 * (global-setup.ts)、自オリジンの Origin と React と同じ X-Requested-With: fetch を付けて送る。
 */
export async function postJson(page: Page, url: string, body: unknown) {
  return page.request.post(url, {
    data: body,
    headers: { Origin: PR_BASE_URL, 'X-Requested-With': 'fetch' },
  });
}

/**
 * EChart 部品 (frontend/src/components/EChart.tsx) の data-testid から、各 series のデータ件数を返す。
 * data-chart-ready="true" (描画完了) を待ってから window.__echarts_getInstanceByDom で取り出す。
 */
export async function getChartSeriesLengths(page: Page, testId: string): Promise<number[]> {
  const el = page.locator(`[data-testid="${testId}"][data-chart-ready="true"]`);
  await expect(el).toHaveCount(1);
  return el.evaluate((dom) => {
    const w = window as unknown as {
      __echarts_getInstanceByDom?: (d: HTMLElement) =>
        | { getOption: () => { series?: { data?: unknown[] }[] } }
        | undefined;
    };
    const inst = w.__echarts_getInstanceByDom?.(dom as HTMLElement);
    if (!inst) throw new Error('echarts instance not found');
    return (inst.getOption().series ?? []).map((s) => (s.data ?? []).length);
  });
}
