// Only failure responses are stubbed. UI guidance is verified independently of
// Rust authorization; real success/image-byte evidence lives in the expansion suite.
import { test, expect } from '@playwright/test';

const cases = [
  { status: 401, code: 'login_required', text: 'ログインが必要です', login: true },
  { status: 403, code: 'job_copy_access_denied', text: '閲覧する権限がありません', login: true },
  { status: 403, code: 'account_disabled', text: 'このアカウントは無効', login: false },
  { status: 404, code: 'moc_not_configured', text: '実データがサーバーに設定されていません', login: false },
  { status: 503, code: 'moc_drive_configuration_invalid', text: '連携設定が不足しています', login: false },
  { status: 503, code: 'drive_listing_configuration_invalid', text: '連携設定が不足しています', login: false },
  { status: 502, code: 'moc_drive_snapshot_unavailable', text: 'Drive上のレビュー用データを読み取れませんでした', login: false },
];

for (const scenario of cases) {
  test(`snapshot ${scenario.status} ${scenario.code} gives actionable guidance without fabricated data`, async ({ page }) => {
    await page.route('**/api/job-copy/moc', route => route.fulfill({
      status: scenario.status, contentType: 'application/json',
      body: JSON.stringify({ code: scenario.code, message: 'PRIVATE_SYNTHETIC_SERVER_DETAIL' }),
    }));
    await page.goto('/app/job-copy');
    const alert = page.getByRole('alert');
    await expect(alert).toContainText(scenario.text);
    await expect(alert).not.toContainText(scenario.code);
    await expect(alert).not.toContainText('PRIVATE_SYNTHETIC_SERVER_DETAIL');
    await expect(page.locator('.jc-job')).toHaveCount(0);
    const login = alert.getByRole('link', { name: '再ログインする', exact: true });
    if (scenario.login) await expect(login).toHaveAttribute('href', '/login');
    else {
      await expect(login).toHaveCount(0);
      await expect(alert).toContainText('管理者');
    }
    await expect(page.locator('.jc-demo')).toContainText('実データ未表示');
  });
}
