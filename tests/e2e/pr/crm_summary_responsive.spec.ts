import { expect, Page, test } from '@playwright/test';
import { login } from './helpers/login';
import { DOCK_STORAGE_KEY } from './helpers/crm_layout';

/**
 * 「案件の概要」の見出しは、パネルの幅 (ビューポートではない) で並べ替わる (queue.css の @container wdsum)。
 *   広い (>= 640px): 3 列 [社名/案件名] [ステージ] [HubSpot/1 行にする]、下に鮮度の行
 *   中 (400-639px): 2 列 [社名/案件名] [ステージ]、下に操作と鮮度
 *   狭い (< 400px): 区切り線つきで 社名 -> ステージ -> 操作+鮮度 と縦に積む
 * 中央の列の幅は保存済みの配置 (widths) で決め、架空の長い社名で実寸を測る。SCREENSHOT_DIR を指定すると画像を残す。
 */
const NAME = '架空サーモスタット工業株式会社';
const SHOT = process.env.SCREENSHOT_DIR;

async function open(page: Page, centerRatio: number): Promise<void> {
  await page.setViewportSize({ width: 1280, height: 900 });
  const layout = JSON.stringify({
    v: 2,
    columns: [
      { panels: ['queue', 'properties'], active: 'queue' },
      { panels: ['overview', 'activity', 'result'], active: 'activity' },
      { panels: ['links'], active: 'links' },
    ],
    widths: [0.2, centerRatio, Math.round((0.8 - centerRatio) * 1000) / 1000],
  });
  await page.addInitScript(([k, v]) => { try { window.localStorage.setItem(k, v); } catch { /* 残せない環境 */ } }, [DOCK_STORAGE_KEY, layout] as const);
  await page.goto('/app/crm?mode=fixture&pipeline=fx-sample');
  await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('架空サンプル');
  await page.getByRole('list', { name: '架電キュー' }).getByRole('button', { name: new RegExp(NAME, 'u') }).first().click();
  await expect(page.getByRole('article', { name: '架電先の詳細' }).locator('h2')).toHaveText(NAME);
  const toExpanded = page.getByRole('button', { name: '詳しく表示' });
  if ((await toExpanded.count()) > 0) await toExpanded.click();
  await expect(page.getByRole('button', { name: '1 行にする' })).toBeVisible();
}

const box = async (page: Page, sel: string) => {
  const b = await page.locator(sel).first().boundingBox();
  if (!b) throw new Error(sel);
  return b;
};

const CASES = [
  { tag: 'narrow', ratio: 0.28, mode: 'narrow' },
  { tag: 'medium', ratio: 0.41, mode: 'medium' },
  { tag: 'wide', ratio: 0.6, mode: 'wide' },
] as const;

test.describe('CRM 架電画面: 案件の概要はパネルの幅で並べ替わる', () => {
  test.beforeEach(async ({ page }) => { await login(page); });

  for (const c of CASES) {
    test(`${c.tag}: 要素の並びと大きさ`, async ({ page }) => {
      await open(page, c.ratio);
      const panel = await box(page, '.wd');
      const name = await box(page, '.wd-s-name h2');
      const deal = await box(page, '.wd-s-name p');
      const stage = await box(page, '.wr-stage-select');
      const actions = await box(page, '.wd-s-actions');
      const fresh = await box(page, '.wd-summary .wd-fresh');
      const summary = await box(page, '.wd-summary');
      const dial = await box(page, '.wd-dialbox');
      const w = Math.round(panel.width);
      console.log(`measure ${c.tag} panel=${String(w)} name=${String(Math.round(name.width))}x${String(Math.round(name.height))} summary=${String(Math.round(summary.height))}`
        + ` stage=(${String(Math.round(stage.x))},${String(Math.round(stage.y))}) actions=(${String(Math.round(actions.x))},${String(Math.round(actions.y))})`);
      if (SHOT) await page.screenshot({ path: `${SHOT}/summary-${c.tag}.png` });

      // 共通: 社名は 1 文字ずつ縦に並ばず (幅 150 以上)、2 行以内。案件名も同様
      expect(name.width).toBeGreaterThanOrEqual(150);
      expect(name.height).toBeLessThanOrEqual(2 * 17 * 1.35 + 2);
      expect(deal.width).toBeGreaterThanOrEqual(150);
      // 「架ける番号」は概要の下に、見えたまま
      await expect(page.locator('.wd-dialbox')).toBeVisible();
      expect(dial.y).toBeGreaterThanOrEqual(summary.y + summary.height - 1);
      // 概要のはみ出しなし
      for (const b of [name, stage, actions, fresh]) expect(b.x + b.width).toBeLessThanOrEqual(panel.x + panel.width + 1);

      if (c.mode === 'wide') {
        expect(w).toBeGreaterThanOrEqual(640);
        expect(Math.abs(stage.y - name.y)).toBeLessThanOrEqual(8);
        expect(stage.x).toBeGreaterThan(name.x + name.width);
        expect(Math.abs(actions.y - name.y)).toBeLessThanOrEqual(8);
        expect(actions.x).toBeGreaterThan(stage.x + stage.width - 1);
        expect(fresh.y).toBeGreaterThan(name.y + name.height - 1);
        expect(summary.height).toBeLessThanOrEqual(150);
      } else if (c.mode === 'medium') {
        expect(w).toBeGreaterThanOrEqual(400);
        expect(w).toBeLessThan(640);
        expect(Math.abs(stage.y - name.y)).toBeLessThanOrEqual(8);
        expect(stage.x).toBeGreaterThan(name.x + name.width);
        expect(actions.y).toBeGreaterThan(name.y + name.height - 1);
        expect(summary.height).toBeLessThanOrEqual(190);
      } else {
        expect(w).toBeLessThan(400);
        expect(stage.y).toBeGreaterThan(deal.y + deal.height - 1);
        expect(stage.width).toBeGreaterThanOrEqual(panel.width * 0.8);
        expect(actions.y).toBeGreaterThan(stage.y + stage.height - 1);
        expect(summary.height).toBeLessThanOrEqual(260);
      }
    });
  }

  test('1 行にする: 1 行表示は従来どおり', async ({ page }) => {
    await open(page, 0.41);
    await page.getByRole('button', { name: '1 行にする' }).click();
    await expect(page.getByTestId('overview-compact')).toBeVisible();
    expect((await box(page, '.wd-line-company')).height).toBeLessThanOrEqual(30);
  });
});
