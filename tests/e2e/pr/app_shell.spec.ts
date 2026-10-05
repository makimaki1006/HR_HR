import { expect, test } from '@playwright/test';
import { E2E_EMAIL } from './helpers/fixture_values';
import { login } from './helpers/login';

// /app/dummy の React 描画には frontend の build (static/app) が要る。
// ナビは旧シェル (dashboard_inline.html) と React (AppShell) が同じ Rust 定義 (src/handlers/nav.rs) から描く。
test.describe('AppShell', () => {
  test('/app/dummy が React で描画され、ヘッダーにログインメールが出る', async ({ page }) => {
    await login(page);
    await page.goto('/app/dummy');
    await expect(page.locator('#app-root header')).toContainText(E2E_EMAIL);
  });

  test('React のナビは /api/nav の hidden=false と同じ並びで、旧シェルのナビとも一致する', async ({ page }) => {
    await login(page);
    // 旧シェル (/) のトップ行 (グループボタンを除く) とサブナビ「調べる」のラベル
    const legacyTop = (
      await page.locator('nav[aria-label="ダッシュボードタブ"] .tab-btn').allInnerTexts()
    ).map((t) => t.trim());
    const legacyExplore = (await page.locator('#explore-subnav .tab-btn').allTextContents()).map((t) =>
      t.trim(),
    );

    const res = await page.request.get('/api/nav', { headers: { Accept: 'application/json' } });
    expect(res.status()).toBe(200);
    const nav = (await res.json()) as {
      items: { id: string; label: string; kind: string; href: string; group: string | null; hidden: boolean }[];
    };
    const visible = nav.items.filter((i) => !i.hidden);
    const topLabels = visible.filter((i) => i.group === null).map((i) => i.label);
    const exploreLabels = visible.filter((i) => i.group === 'explore').map((i) => i.label);
    // fixture のサーバは GEMINI/キーワード系 env 無し・非 admin なので、隠しタブ・CRM・求人票作成は出ない
    expect(topLabels).toEqual(['媒体分析', '競合調査', '営業KPI', 'コンサルKPI', '求人文面（MOC）']);
    const jobCopy = visible.filter((item) => item.id === 'job-copy');
    expect(jobCopy).toEqual([expect.objectContaining({ kind: 'app', href: '/app/job-copy', group: null })]);
    const legacyJobCopy = page.locator('nav[aria-label="ダッシュボードタブ"] a[href="/app/job-copy"]');
    await expect(legacyJobCopy).toHaveCount(1);
    await expect(legacyJobCopy).toHaveText('求人文面（MOC）');
    expect(exploreLabels).toEqual(['地図', '地域分析', '企業検索', '職種辞典', '資格辞書', '採用市場']);
    expect(visible.map((i) => i.label)).not.toContain('市場概況');

    await page.goto('/app/dummy');
    const reactTop = page.locator('#app-root nav.hr-nav:not(.hr-subnav) a[data-nav-id]');
    await expect(reactTop).toHaveText(topLabels);
    await expect(reactTop.filter({ hasText: '求人文面（MOC）' })).toHaveCount(1);
    await expect(page.locator('#app-root a[data-nav-id="job-copy"]')).toHaveAttribute('href', '/app/job-copy');
    await page.locator('#app-root button[data-group-id="explore"]').click();
    await expect(page.locator('#hr-subnav a[data-nav-id]')).toHaveText(exploreLabels);
    // 旧シェルも同じ定義から描いているので、ラベルの並びが一致する
    expect(legacyTop).toEqual(topLabels);
    expect(legacyExplore).toEqual(exploreLabels);
  });

  test('/app/dummy の React シェルが配信される (未ビルドの注記ではない)', async ({ page }) => {
    await login(page);
    const res = await page.goto('/app/dummy');
    expect(res?.status()).toBe(200);
    await expect(page.locator('#app-root')).toBeAttached();
    await expect(page.getByRole('heading', { name: 'フロントエンド未ビルド' })).toHaveCount(0);
  });
});
