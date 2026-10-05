import { expect, test } from '@playwright/test';
import { FIXTURE, E2E_EMAIL } from './helpers/fixture_values';
import { login } from './helpers/login';

test.describe('smoke', () => {
  test('/health は fixture の postings 行数を返す', async ({ request }) => {
    const r = await request.get('/health');
    expect(r.ok()).toBeTruthy();
    const body = await r.json();
    expect(body.db_connected).toBe(true);
    expect(body.db_rows).toBe(FIXTURE.postingsTotal); // 63 (make_fixture_db.py)
  });

  test('ログイン後のダッシュボードに実在するナビ項目が並ぶ', async ({ page }) => {
    await login(page);
    const header = page.getByText(`ログイン: ${E2E_EMAIL}`);
    await expect(header).toBeVisible();
    // templates/dashboard_inline.html のトップ段 (キーワード需要/求人票作成は env 依存なので対象外)
    const nav = page.locator('nav[aria-label="ダッシュボードタブ"]');
    await expect(nav.locator('.tab-btn, .tab-btn-group')).toHaveText(['媒体分析', '競合調査', '調べる ▾', '営業KPI', 'コンサルKPI', '求人文面（MOC）']);
    await expect(nav.locator('a[href="/app/job-copy"]')).toHaveCount(1);
    await expect(nav.locator('a[href="/app/job-copy"]')).toHaveText('求人文面（MOC）');
    // 「調べる」配下 (初期は非表示だが DOM にある)
    const sub = page.locator('#explore-subnav .tab-btn');
    await expect(sub).toHaveText(['地図', '地域分析', '企業検索', '職種辞典', '資格辞書', '採用市場']);
    // 旧「市場概況」タブは UI に無い
    await expect(page.getByRole('tab', { name: '市場概況' })).toHaveCount(0);
  });

  test('未ログインで /api/nav (Accept: application/json) は 401 auth_required', async ({ request }) => {
    const r = await request.get('/api/nav', { headers: { Accept: 'application/json' }, maxRedirects: 0 });
    expect(r.status()).toBe(401);
    expect((await r.json()).error).toBe('auth_required');
  });

  test('未ログインの求人文面APIは設定を公開せず401JSONを返す', async ({ request }) => {
    for (const path of ['/api/job-copy/moc', '/api/job-copy/image?company_id=10&listing_id=30&manifest_id=40&slot=1']) {
      const response = await request.get(path, { headers: { Accept: 'application/json' }, maxRedirects: 0 });
      expect(response.status()).toBe(401);
      expect(response.headers()['cache-control']).toBe('no-store');
      expect(response.headers()['content-type']).toContain('application/json');
      expect(await response.json()).toEqual({ code: 'login_required' });
    }
  });

  test('求人文面の導線はReact画面へ移動しパスワードログインでは実データを拒否する', async ({ page }) => {
    await login(page);
    await page.locator('nav[aria-label="ダッシュボードタブ"] a[href="/app/job-copy"]').click();
    await expect(page).toHaveURL(/\/app\/job-copy$/);
    await expect(page.locator('#app-root header').first()).toContainText(E2E_EMAIL);
    await expect(page.locator('#app-root a[data-nav-id="job-copy"]')).toHaveAttribute('href', '/app/job-copy');
    for (const path of ['/api/job-copy/moc', '/api/job-copy/image?company_id=10&listing_id=30&manifest_id=40&slot=1']) {
      const response = await page.request.get(path, { headers: { Accept: 'application/json' }, maxRedirects: 0 });
      expect(response.status()).toBe(403);
      expect(response.headers()['cache-control']).toBe('no-store');
      expect(await response.json()).toEqual({ code: 'job_copy_access_denied' });
    }
  });
});
