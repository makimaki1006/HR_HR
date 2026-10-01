import { expect, test } from '@playwright/test';
import { E2E_EMAIL } from './helpers/fixture_values';
import { login } from './helpers/login';

// /app/dummy の React 描画には frontend の build (static/app) が要る。
// AppShell と /api/nav は platform-team が並行実装中で、この worktree には無い。
// 実装後に fixme を外し、セレクタ (header / nav 項目) を AppShell の実際の DOM に合わせる。
test.describe('AppShell', () => {
  test('/app/dummy が React で描画され、ヘッダーにログインメールが出る', async ({ page }) => {
    test.fixme(true, 'AppShell 未実装 (d62c53d に無い)。実装後に外す');
    await login(page);
    await page.goto('/app/dummy');
    await expect(page.locator('#app-root header')).toContainText(E2E_EMAIL);
  });

  test('ナビ項目数が /api/nav の hidden=false の件数と一致する', async ({ page }) => {
    test.fixme(true, '/api/nav 未実装。実装後、レスポンスの items.filter(i => !i.hidden).length と AppShell の nav 項目数を比べる');
    await login(page);
    const res = await page.request.get('/api/nav', { headers: { Accept: 'application/json' } });
    const nav = await res.json();
    const visible = (nav.items ?? nav).filter((i: { hidden: boolean }) => !i.hidden).length;
    await page.goto('/app/dummy');
    await expect(page.locator('#app-root nav a')).toHaveCount(visible);
  });

  test('/app/dummy の React シェルが配信される (未ビルドの注記ではない)', async ({ page }) => {
    await login(page);
    const res = await page.goto('/app/dummy');
    expect(res?.status()).toBe(200);
    await expect(page.locator('#app-root')).toBeAttached();
    await expect(page.getByRole('heading', { name: 'フロントエンド未ビルド' })).toHaveCount(0);
  });
});
