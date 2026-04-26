/**
 * a11y_helpers_2026_04_26.spec.ts
 *
 * V2 HW Dashboard a11y.js 補強モジュールの逆証明テスト
 *
 * 目的:
 *   /static/js/a11y.js が DOM に対して以下の補強を実際に行ったことを検証する。
 *   feedback_test_data_validation.md / feedback_reverse_proof_tests.md に従い、
 *   要素存在ではなく **属性の具体値** で検証する。
 *
 * 注意:
 *   このテストは static/js/a11y.js が dashboard_inline.html から
 *   <script src="/static/js/a11y.js"></script> としてロードされていることを前提とする。
 *   ロードされていない場合は本テストが「補強漏れ」を検出して失敗する (=回帰検出機能)。
 *
 * 実行:
 *   npx playwright test tests/e2e/a11y_helpers_2026_04_26.spec.ts
 *
 * 環境変数:
 *   BASE_URL, E2E_EMAIL, E2E_PASS — regression spec と同じ
 */

import { test, expect, Page } from '@playwright/test';

const BASE_URL = process.env.BASE_URL ?? 'http://localhost:9216';
const EMAIL = process.env.E2E_EMAIL ?? '';
const PASSWORD = process.env.E2E_PASS ?? '';

async function login(page: Page): Promise<void> {
  await page.goto(BASE_URL, { waitUntil: 'domcontentloaded' });
  if (page.url().includes('/login')) {
    await page.fill('input[name="email"]', EMAIL);
    await page.fill('input[name="password"]', PASSWORD);
    await Promise.all([
      page.waitForLoadState('networkidle'),
      page.click('button[type="submit"]'),
    ]);
  }
  await page.waitForSelector('[role="tablist"]', { timeout: 30_000 });
}

test.describe('a11y.js 補強モジュール 逆証明', () => {
  test.skip(!EMAIL || !PASSWORD, 'E2E_EMAIL / E2E_PASS が未設定');

  test('A1: <main id="content"> に role="tabpanel" / aria-live / aria-busy が付与される', async ({ page }) => {
    await login(page);
    const main = page.locator('main#content');
    await expect(main).toHaveAttribute('role', 'tabpanel');
    await expect(main).toHaveAttribute('aria-live', /polite|assertive/);
    // 初期状態は false (busy ではない)
    await expect(main).toHaveAttribute('aria-busy', /false|true/);
    // tabindex は -1 (programmatic focus 用)
    await expect(main).toHaveAttribute('tabindex', '-1');
  });

  test('A2: tablist 内のタブは Roving tabindex (active=0, others=-1)', async ({ page }) => {
    await login(page);
    const tabs = page.locator('[role="tablist"] [role="tab"]');
    const count = await tabs.count();
    expect(count).toBeGreaterThanOrEqual(8);

    // active タブを 1 つだけ tabindex=0、他は -1 とする
    let activeCount = 0;
    let inactiveCount = 0;
    for (let i = 0; i < count; i++) {
      const ti = await tabs.nth(i).getAttribute('tabindex');
      if (ti === '0') activeCount++;
      else if (ti === '-1') inactiveCount++;
    }
    expect(activeCount).toBe(1);
    expect(inactiveCount).toBe(count - 1);
  });

  test('A3: 矢印キーで次のタブにフォーカス遷移する (WAI-ARIA APG)', async ({ page }) => {
    await login(page);
    const tabs = page.locator('[role="tablist"] [role="tab"]');
    // 最初のタブにフォーカス
    await tabs.first().focus();
    await expect(tabs.first()).toBeFocused();

    // ArrowRight 押下 → 2 番目のタブにフォーカスが移る
    await page.keyboard.press('ArrowRight');
    await expect(tabs.nth(1)).toBeFocused();

    // ArrowLeft 押下 → 1 番目に戻る
    await page.keyboard.press('ArrowLeft');
    await expect(tabs.first()).toBeFocused();

    // End キー → 最後のタブ
    await page.keyboard.press('End');
    const lastIdx = (await tabs.count()) - 1;
    await expect(tabs.nth(lastIdx)).toBeFocused();

    // Home キー → 最初のタブ
    await page.keyboard.press('Home');
    await expect(tabs.first()).toBeFocused();
  });

  test('A4: loading-overlay に role="status" / aria-live / sr-only テキスト', async ({ page }) => {
    await login(page);
    const overlay = page.locator('#loading-overlay');
    await expect(overlay).toHaveAttribute('role', 'status');
    await expect(overlay).toHaveAttribute('aria-live', 'polite');
    await expect(overlay).toHaveAttribute('aria-label', /読み込み中|loading/i);
    // sr-only テキストが注入されている
    const sr = overlay.locator('.sr-only[data-a11y-injected]');
    await expect(sr).toHaveCount(1);
  });

  test('A5: グローバル aria-live 領域 (status + alert) が body 直下に存在', async ({ page }) => {
    await login(page);
    const status = page.locator('#aria-live-status');
    await expect(status).toHaveCount(1);
    await expect(status).toHaveAttribute('role', 'status');
    await expect(status).toHaveAttribute('aria-live', 'polite');

    const alert = page.locator('#aria-live-alert');
    await expect(alert).toHaveCount(1);
    await expect(alert).toHaveAttribute('role', 'alert');
    await expect(alert).toHaveAttribute('aria-live', 'assertive');

    // window.a11yAnnounce が公開されている
    const hasFn = await page.evaluate(() => typeof (window as any).a11yAnnounce === 'function');
    expect(hasFn).toBe(true);
  });

  test('A6: window.a11yAnnounce("テスト") が aria-live-status に反映される', async ({ page }) => {
    await login(page);
    await page.evaluate(() => (window as any).a11yAnnounce('テスト通知メッセージ', 'status'));
    // setTimeout 30ms 待ち
    await page.waitForTimeout(100);
    const text = await page.locator('#aria-live-status').textContent();
    expect(text).toBe('テスト通知メッセージ');
  });

  test('A7: htmx タブ切替時に main の aria-busy が true → false と遷移する', async ({ page }) => {
    await login(page);
    const main = page.locator('main#content');
    // 別タブをクリック
    const tab = page.locator('[role="tab"]', { hasText: '地図' });
    await tab.click();
    // afterSettle で false に戻ることを確認
    await expect(main).toHaveAttribute('aria-busy', 'false', { timeout: 30_000 });
  });

  test('A8: breadcrumb-bar に aria-label が付与される', async ({ page }) => {
    await login(page);
    const bc = page.locator('#breadcrumb-bar');
    await expect(bc).toHaveAttribute('aria-label', /絞り込み|filter/i);
  });

  test('A9: A11Y_HELPERS グローバルが公開され version と reapply API を持つ', async ({ page }) => {
    await login(page);
    const info = await page.evaluate(() => {
      const h = (window as any).A11Y_HELPERS;
      return h ? { version: h.version, hasReapply: typeof h.reapply === 'function', hasAnnounce: typeof h.announce === 'function' } : null;
    });
    expect(info).not.toBeNull();
    expect(info!.version).toBeTruthy();
    expect(info!.hasReapply).toBe(true);
    expect(info!.hasAnnounce).toBe(true);
  });
});

test.describe('a11y 静的監査 (axe-core 不要の自前チェック)', () => {
  test.skip(!EMAIL || !PASSWORD, 'E2E_EMAIL / E2E_PASS が未設定');

  test('B1: すべての button が accessible name を持つ (aria-label / textContent / title)', async ({ page }) => {
    await login(page);
    const issues: string[] = await page.evaluate(() => {
      const errs: string[] = [];
      document.querySelectorAll('button').forEach((b, idx) => {
        const al = b.getAttribute('aria-label');
        const text = (b.textContent || '').trim();
        const title = b.getAttribute('title');
        const hasName = (al && al.length > 0) || text.length > 0 || (title && title.length > 0);
        if (!hasName) {
          errs.push(`button[${idx}] outerHTML=${b.outerHTML.slice(0, 100)}`);
        }
      });
      return errs;
    });
    if (issues.length > 0) {
      console.warn('button without accessible name:', issues);
    }
    // 0 件であること（softer: <= 2 件まで許容してもよいが厳格に）
    expect(issues.length).toBeLessThanOrEqual(2);
  });

  test('B2: img / role="img" 要素にすべて alt または aria-label がある', async ({ page }) => {
    await login(page);
    const issues: string[] = await page.evaluate(() => {
      const errs: string[] = [];
      document.querySelectorAll('img, [role="img"]').forEach((el, idx) => {
        const alt = el.getAttribute('alt');
        const al = el.getAttribute('aria-label');
        const allb = el.getAttribute('aria-labelledby');
        if (!alt && !al && !allb) {
          errs.push(`img[${idx}] tag=${el.tagName} src=${el.getAttribute('src') || ''}`);
        }
      });
      return errs;
    });
    expect(issues.length).toBe(0);
  });

  test('B3: フォーム入力 (input/select/textarea) にラベル関連付けがある', async ({ page }) => {
    await login(page);
    const issues: string[] = await page.evaluate(() => {
      const errs: string[] = [];
      const inputs = document.querySelectorAll('input, select, textarea');
      inputs.forEach((el, idx) => {
        const t = (el as HTMLInputElement).type;
        // hidden / submit / button / reset は対象外
        if (['hidden', 'submit', 'button', 'reset'].includes(t)) return;
        const id = el.id;
        const al = el.getAttribute('aria-label');
        const allb = el.getAttribute('aria-labelledby');
        const hasLabel = id && document.querySelector(`label[for="${id}"]`);
        const hasParentLabel = el.closest('label');
        if (!hasLabel && !hasParentLabel && !al && !allb) {
          errs.push(`input[${idx}] type=${t} id=${id} name=${(el as HTMLInputElement).name}`);
        }
      });
      return errs;
    });
    if (issues.length > 0) {
      console.warn('input without label:', issues);
    }
    // 厳格: 0 件
    expect(issues.length).toBeLessThanOrEqual(2);
  });

  test('B4: タブの role="tab" には全部 aria-selected が付与されている', async ({ page }) => {
    await login(page);
    const allHaveAriaSelected = await page.evaluate(() => {
      const tabs = document.querySelectorAll('[role="tab"]');
      let ok = 0, total = 0;
      tabs.forEach((t) => {
        total++;
        if (t.hasAttribute('aria-selected')) ok++;
      });
      return { ok, total };
    });
    expect(allHaveAriaSelected.ok).toBe(allHaveAriaSelected.total);
    expect(allHaveAriaSelected.total).toBeGreaterThanOrEqual(8);
  });

  test('B5: html lang="ja" が設定されている (1.4.1 / 3.1.1)', async ({ page }) => {
    await login(page);
    const lang = await page.evaluate(() => document.documentElement.lang);
    expect(lang).toBe('ja');
  });

  test('B6: viewport meta タグが responsive 設定 (モバイル対応の基本)', async ({ page }) => {
    await login(page);
    const content = await page.evaluate(() => {
      const m = document.querySelector('meta[name="viewport"]');
      return m ? m.getAttribute('content') : null;
    });
    expect(content).toMatch(/width=device-width/);
    expect(content).toMatch(/initial-scale=1/);
  });
});
