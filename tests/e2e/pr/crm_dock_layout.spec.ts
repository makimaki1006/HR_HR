import { expect, Page, test } from '@playwright/test';
import { login } from './helpers/login';
import { DOCK_STORAGE_KEY } from './helpers/crm_layout';

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

    // 既定: 左 = 架電一覧 (前) + プロパティ、中央 = 案件の概要 (上端に固定) + 活動ログ (前) / 架電結果の入力 / 求人検索・リンク先、右 = 空き
    await expect(tablist(page, '左').getByRole('tab')).toHaveText(['架電一覧', 'プロパティ']);
    await expect(tablist(page, '左').getByRole('tab', { name: '架電一覧' })).toHaveAttribute('aria-selected', 'true');
    await expect(tablist(page, '中央').getByRole('tab')).toHaveText(['活動ログ', '架電結果の入力', '求人検索・リンク先']);
    await expect(page.getByRole('region', { name: '活動ログ' })).toBeVisible();
    await expect(page.getByRole('region', { name: '活動ログ' }).locator('.wd-act-call .wd-act-meta').first()).toHaveText('発信 · 完了 · 通話時間 1分05秒');
    await expect(page.getByRole('region', { name: '右の列(空き)' })).toBeVisible();
    await expect(separator(page)).toHaveAttribute('aria-valuenow', '26');

    // プロパティ: 表示名と値 (日付は年月日、空は「未入力」、URL はリンク)
    await tablist(page, '左').getByRole('tab', { name: 'プロパティ' }).click();
    const props = page.getByTestId('property-panel');
    const value = (label: string) => props.locator('dt', { hasText: label }).locator('xpath=following-sibling::dd[1]');
    await expect(value('次回架電日')).toHaveText('2026/09/30');
    await expect(value('最終架電日')).toHaveText('2026/09/25');
    await expect(value('架電禁止理由')).toHaveText('未入力');
    await expect(value('姓')).toHaveText('丁野');
    await expect(props).not.toContainText('bpo_');

    // メニューで「活動ログ」を左へ
    await tablist(page, '中央').getByRole('tab', { name: '活動ログ' }).click();
    await page.getByRole('button', { name: '「活動ログ」の移動' }).click();
    const menu = page.getByRole('menu', { name: '「活動ログ」の移動' });
    await expect(menu.getByRole('menuitem')).toHaveText(['左へ移動', '右へ移動', 'この列で後ろへ']);
    await menu.getByRole('menuitem', { name: '左へ移動' }).click();
    await expect(tablist(page, '左').getByRole('tab')).toHaveText(['架電一覧', 'プロパティ', '活動ログ']);
    await expect(tablist(page, '左').getByRole('tab', { name: '活動ログ' })).toHaveAttribute('aria-selected', 'true');
    await expect(tablist(page, '左').getByRole('tab', { name: '活動ログ' })).toBeFocused();
    await expect(tablist(page, '中央').getByRole('tab')).toHaveText(['架電結果の入力', '求人検索・リンク先']);
    await expect(page.getByRole('region', { name: '活動ログ' })).toBeVisible();
    // 架電結果の入力が中央の前に出て、入力欄が見えている
    await expect(page.getByRole('form', { name: '架電結果の入力' })).toBeVisible();

    // 区切りを矢印キーで右へ (左の列が広がる)
    const leftBefore = await colWidth(page, 0);
    await separator(page).focus();
    for (let i = 0; i < 3; i += 1) await page.keyboard.press('ArrowRight');
    await expect(separator(page)).toHaveAttribute('aria-valuenow', '32');
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
    await expect(tablist(page, '中央').getByRole('tab')).toHaveText(['活動ログ', '架電結果の入力', '求人検索・リンク先']);
    await expect(separator(page)).toHaveAttribute('aria-valuenow', '26');

    // 架空サンプルでは /api/crm/* を一度も呼ばない (配置の変更でも)
    expect(crm).toEqual([]);
  });

  test('壊れた配置が残っていても既定の配置で開く', async ({ page }) => {
    await page.addInitScript((k) => { window.localStorage.setItem(k, '{"v":1,"columns":[{"panels":["queue","queue"]}'); }, DOCK_STORAGE_KEY);
    await openFirst(page);
    await expect(tablist(page, '左').getByRole('tab')).toHaveText(['架電一覧', 'プロパティ']);
    await expect(tablist(page, '中央').getByRole('tab')).toHaveText(['活動ログ', '架電結果の入力', '求人検索・リンク先']);
  });
});
