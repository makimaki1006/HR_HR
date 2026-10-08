import { expect, Page, test } from '@playwright/test';
import { login } from './helpers/login';
import { DOCK_STORAGE_KEY } from './helpers/crm_layout';

/** 以前の版 (v1) の配置の置き場所 (dockModel.LEGACY_DOCK_STORAGE_KEY) */
const LEGACY_DOCK_STORAGE_KEY = 'hrhr.crm.dockLayout.v1';

/**
 * 架電画面 (/app/crm?mode=fixture) のパネルの配置: 3 列、列ごとにタブ。移動・幅の変更・再読み込みでの復元。
 *
 * - 架空サンプルで動かす (/api/crm/* は呼ばない。呼んだら失敗にする)。値は queueFixture.ts / workspaceFixture.ts / usePropertyCatalog.ts の固定値
 * - 配置はこのブラウザの localStorage に残る (frontend/src/screens/crm/dockModel.ts)
 * - この画面にグラフ (ECharts) は無い。値は描画された文字・属性・要素の幅で確かめる
 */

const SCREEN_URL = '/app/crm?mode=fixture';

const tablist = (page: Page, col: '左' | '中央' | '右') => page.getByRole('tablist', { name: `${col}の列のパネル` });
const separator = (page: Page) => page.getByRole('separator', { name: '左の列と中央の列の幅' });
const colWidth = (page: Page, i: number) => page.getByTestId(`dock-col-${String(i)}`).evaluate((el) => Math.round(el.getBoundingClientRect().width));

async function openFirst(page: Page): Promise<void> {
  await page.goto(SCREEN_URL);
  await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('架空サンプル');
  const list = page.getByRole('list', { name: '架電キュー' });
  await expect(list.locator('.cq-row-company').first()).toHaveText('ダミー建設');
  await list.locator('li.cq-row button.cq-row-button').first().click();
  await expect(page.getByRole('article', { name: '架電先の詳細' }).locator('h2')).toHaveText('ダミー建設');
}

test.describe('CRM 架電画面: パネルの配置', () => {
  test.beforeEach(async ({ page }) => { await login(page); });

  test('既定の配置 → 活動ログを左へ移し、列の幅を変え、再読み込みしても残る。元の配置に戻せる。HubSpot は呼ばない', async ({ page }) => {
    const crm: string[] = [];
    page.on('request', (r) => { if (new URL(r.url()).pathname.startsWith('/api/crm/')) crm.push(r.url()); });
    await page.setViewportSize({ width: 1440, height: 900 });
    await openFirst(page);

    // 既定 (v2): 左 = 架電一覧 (前) + プロパティ、中央 = 案件の概要 (上端に固定) + 活動ログ (前) / 架電結果の入力、右 = 求人検索・リンク先
    await expect(tablist(page, '左').getByRole('tab')).toHaveText(['架電一覧', 'プロパティ']);
    await expect(tablist(page, '左').getByRole('tab', { name: '架電一覧' })).toHaveAttribute('aria-selected', 'true');
    await expect(tablist(page, '中央').getByRole('tab')).toHaveText(['活動ログ', '架電結果の入力']);
    await expect(tablist(page, '右').getByRole('tab')).toHaveText(['求人検索・リンク先']);
    await expect(page.getByRole('region', { name: '活動ログ' })).toBeVisible();
    await expect(page.getByRole('region', { name: '活動ログ' }).locator('.wd-act-call .wd-act-meta').first()).toHaveText('発信 · 完了 · 通話時間 1分05秒');
    await expect(page.getByRole('region', { name: '右の列(空き)' })).toHaveCount(0);
    await expect(separator(page)).toHaveAttribute('aria-valuenow', '38');
    // 1440px で 左 ≈ 300px、右 ≈ 640px
    expect(Math.abs((await colWidth(page, 0)) - 300)).toBeLessThanOrEqual(4);
    expect(Math.abs((await colWidth(page, 2)) - 643)).toBeLessThanOrEqual(4);

    // プロパティ: HubSpot のカード「リスト情報」(開いている) の表示名と値 (日付は年月日、空は「未入力」)
    await tablist(page, '左').getByRole('tab', { name: 'プロパティ' }).click();
    const props = page.getByTestId('property-panel');
    const value = (label: string) => props.locator('dt').filter({ hasText: new RegExp(`^${label}$`, 'u') }).locator('xpath=following-sibling::dd[1]');
    await expect(value('架電日')).toHaveText('2026/09/25');
    await expect(value('再架電日')).toHaveText('2026/09/30');
    await expect(value('再架電時間')).toHaveText('16:00');
    await expect(value('利用中サービス')).toHaveText('未入力');
    await expect(value('担当者名')).toHaveText('丁野 三郎');
    await expect(props).not.toContainText('bpo_');
    await expect(props).not.toContainText('risuto_');

    // メニューで「活動ログ」を左へ
    await tablist(page, '中央').getByRole('tab', { name: '活動ログ' }).click();
    await page.getByRole('button', { name: '「活動ログ」の移動' }).click();
    const menu = page.getByRole('menu', { name: '「活動ログ」の移動' });
    await expect(menu.getByRole('menuitem')).toHaveText(['左へ移動', '右へ移動', 'この列で後ろへ']);
    await menu.getByRole('menuitem', { name: '左へ移動' }).click();
    await expect(tablist(page, '左').getByRole('tab')).toHaveText(['架電一覧', 'プロパティ', '活動ログ']);
    await expect(tablist(page, '左').getByRole('tab', { name: '活動ログ' })).toHaveAttribute('aria-selected', 'true');
    await expect(tablist(page, '左').getByRole('tab', { name: '活動ログ' })).toBeFocused();
    await expect(tablist(page, '中央').getByRole('tab')).toHaveText(['架電結果の入力']);
    await expect(page.getByRole('region', { name: '活動ログ' })).toBeVisible();
    // 架電結果の入力が中央の前に出て、入力欄が見えている
    await expect(page.getByRole('form', { name: '架電結果の入力' })).toBeVisible();

    // 区切りを矢印キーで右へ (左の列が広がる)
    const leftBefore = await colWidth(page, 0);
    await separator(page).focus();
    for (let i = 0; i < 3; i += 1) await page.keyboard.press('ArrowRight');
    await expect(separator(page)).toHaveAttribute('aria-valuenow', '49');
    const leftAfter = await colWidth(page, 0);
    expect(leftAfter).toBeGreaterThan(leftBefore + 50);

    // ドラッグでも幅が変わる (左へ 80px)
    const box = await separator(page).boundingBox();
    if (!box) throw new Error('separator');
    await page.mouse.move(box.x + box.width / 2, box.y + 200);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 - 80, box.y + 200, { steps: 4 });
    await page.mouse.up();
    const leftDragged = await colWidth(page, 0);
    expect(Math.abs(leftAfter - 80 - leftDragged)).toBeLessThanOrEqual(6);
    const pct = await separator(page).getAttribute('aria-valuenow');

    // 再読み込み: 配置と幅が残る (このブラウザの localStorage)
    const stored = await page.evaluate((k) => window.localStorage.getItem(k), DOCK_STORAGE_KEY);
    expect(stored).toContain('"activity"');
    await page.reload();
    await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('架空サンプル');
    await expect(tablist(page, '左').getByRole('tab')).toHaveText(['架電一覧', 'プロパティ', '活動ログ']);
    await expect(separator(page)).toHaveAttribute('aria-valuenow', pct ?? '');
    expect(Math.abs((await colWidth(page, 0)) - leftDragged)).toBeLessThanOrEqual(2);

    // 元の配置に戻す
    await page.getByRole('button', { name: '元の配置に戻す' }).click();
    await expect(tablist(page, '左').getByRole('tab')).toHaveText(['架電一覧', 'プロパティ']);
    await expect(tablist(page, '中央').getByRole('tab')).toHaveText(['活動ログ', '架電結果の入力']);
    await expect(tablist(page, '右').getByRole('tab')).toHaveText(['求人検索・リンク先']);
    await expect(separator(page)).toHaveAttribute('aria-valuenow', '38');

    // 架空サンプルでは /api/crm/* を一度も呼ばない (配置の変更でも)
    expect(crm).toEqual([]);
  });

  test('壊れた配置が残っていても既定の配置で開く', async ({ page }) => {
    await page.addInitScript((k) => { window.localStorage.setItem(k, '{"v":2,"columns":[{"panels":["queue","queue"]}'); }, DOCK_STORAGE_KEY);
    await openFirst(page);
    await expect(tablist(page, '左').getByRole('tab')).toHaveText(['架電一覧', 'プロパティ']);
    await expect(tablist(page, '中央').getByRole('tab')).toHaveText(['活動ログ', '架電結果の入力']);
    await expect(tablist(page, '右').getByRole('tab')).toHaveText(['求人検索・リンク先']);
  });

  test('以前の版 (v1) の配置が残っていたら、1 回だけ新しい既定の配置にして案内する', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    const v1 = JSON.stringify({ v: 1, columns: [{ panels: ['queue', 'properties'], active: 'queue' }, { panels: ['overview', 'activity', 'result', 'links'], active: 'result' }, { panels: [], active: null }], widths: [0.26, 0.74, 0.3] });
    await page.goto('/app/crm?mode=fixture');
    await page.evaluate(([k, v]) => { window.localStorage.removeItem('hrhr.crm.dockLayout.v2'); window.localStorage.setItem(k, v); }, [LEGACY_DOCK_STORAGE_KEY, v1] as const);
    await openFirst(page);
    await expect(page.getByTestId('layout-note')).toContainText('画面の配置を更新しました。「求人検索・リンク先」を右の列に移し');
    await expect(tablist(page, '右').getByRole('tab')).toHaveText(['求人検索・リンク先']);
    expect(await page.evaluate((k) => window.localStorage.getItem(k), LEGACY_DOCK_STORAGE_KEY)).toBeNull();
    await page.reload();
    await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('架空サンプル');
    await expect(page.getByTestId('layout-note')).toHaveCount(0);
  });

  test('1440x900: 求人検索の枠は 600x560 以上。「広げる」で置き場全体に広がり (枠は読み直さない)、Esc で戻る。1366x768 では案件の概要が 1 行', async ({ page }) => {
    const external: string[] = [];
    await page.route(/^https:\/\/www\.google\.com\//, async (route) => {
      external.push(route.request().url());
      await route.fulfill({ status: 200, contentType: 'text/html; charset=utf-8', body: '<!doctype html><title>stub</title><p id="stub">外部ページ(テスト用の代わり)</p>' });
    });
    await page.setViewportSize({ width: 1440, height: 900 });
    await openFirst(page);
    // 背の高い画面では案件の概要は詳しい表示
    await expect(page.getByTestId('overview-compact')).toHaveCount(0);
    await page.getByRole('region', { name: 'リンク' }).getByRole('link', { name: /求人を検索する/ }).click();
    const frame = page.locator('#cq-cpanel-search iframe[data-testid="link-frame"]');
    await expect(frame).toBeVisible();
    const box = await frame.boundingBox();
    if (!box) throw new Error('frame');
    expect(box.width).toBeGreaterThanOrEqual(600);
    expect(box.height).toBeGreaterThanOrEqual(560);
    await expect(page.frameLocator('#cq-cpanel-search iframe').locator('#stub')).toBeVisible();
    const loads = external.length;
    // 同じ要素のままかを見る印
    await frame.evaluate((el) => { el.setAttribute('data-mark', 'kept'); });

    await page.getByTestId('links-maximize').click();
    await expect(page.getByTestId('links-maximize')).toHaveText('戻す');
    await expect(page.getByTestId('dock-col-2')).toHaveClass(/is-max/);
    const big = await frame.boundingBox();
    if (!big) throw new Error('frame');
    expect(big.width).toBeGreaterThan(1300);
    expect(big.height).toBeGreaterThanOrEqual(box.height);
    await expect(frame).toHaveAttribute('data-mark', 'kept');
    // 広げている間は、ほかの列を操作できない
    await expect(page.getByTestId('dock-col-0')).toHaveAttribute('inert', '');
    await page.keyboard.press('Escape');
    await expect(page.getByTestId('dock-col-2')).not.toHaveClass(/is-max/);
    await expect(page.getByTestId('links-maximize')).toHaveText('広げる');
    await expect(frame).toHaveAttribute('data-mark', 'kept');
    expect(Math.round((await frame.boundingBox())?.width ?? 0)).toBe(Math.round(box.width));
    // 広げる・戻すで枠の中のページを読み直していない
    expect(external.length).toBe(loads);

    // 背の低い画面では、案件の概要は 1 行 (会社 · 担当者 · 番号 · 発信)
    await page.setViewportSize({ width: 1366, height: 768 });
    const compact = page.getByTestId('overview-compact');
    await expect(compact).toBeVisible();
    await expect(compact.locator('h2')).toHaveText('ダミー建設');
    await expect(compact.locator('.cq-phone')).toHaveText('03-0000-0005');
    await expect(compact.getByRole('button', { name: /に発信$/ })).toHaveText('発信');
    await compact.getByRole('button', { name: '詳しく表示' }).click();
    await expect(page.getByRole('region', { name: '架ける番号' })).toBeVisible();
  });
});
