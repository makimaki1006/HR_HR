import { expect, Page, test } from '@playwright/test';
import { login } from './helpers/login';
import { DOCK_STORAGE_KEY } from './helpers/crm_layout';

/**
 * 架電画面 (/app/crm?mode=fixture) の「案件の概要」の見出し: 長い社名が 1 文字ずつ縦に並ばない。
 * 架空サンプル f-17「架空サーモスタット工業株式会社」(frontend/src/screens/crm/queueFixture.ts) を、
 * 既定の配置と、中央の列を狭めた配置 (1280x720) で開き、社名の要素の実寸と概要の高さを測る。
 * SCREENSHOT_DIR を指定すると、確認用の画像を残す。
 */

const SCREEN_URL = '/app/crm?mode=fixture&pipeline=fx-sample';
const NAME = '架空サーモスタット工業株式会社';
const SHOT = process.env.SCREENSHOT_DIR;

const NARROW = JSON.stringify({
  v: 2,
  columns: [
    { panels: ['queue', 'properties'], active: 'queue' },
    { panels: ['overview', 'activity', 'result'], active: 'activity' },
    { panels: ['links'], active: 'links' },
  ],
  widths: [0.34, 0.3, 0.36],
});

async function open(page: Page, narrow: boolean): Promise<void> {
  await page.setViewportSize({ width: 1280, height: 720 });
  if (narrow) {
    await page.addInitScript(([k, v]) => { try { window.localStorage.setItem(k, v); } catch { /* 残せない環境 */ } }, [DOCK_STORAGE_KEY, NARROW] as const);
  }
  await page.goto(SCREEN_URL);
  await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('架空サンプル');
  await page.getByRole('list', { name: '架電キュー' }).getByRole('button', { name: new RegExp(NAME, 'u') }).first().click();
  await expect(page.getByRole('article', { name: '架電先の詳細' }).locator('h2')).toHaveText(NAME);
}

/** 低い画面では 1 行表示で始まる。「詳しく表示」/「1 行にする」で目的の表示にそろえる */
async function setDensity(page: Page, want: 'expanded' | 'compact'): Promise<void> {
  const toExpanded = page.getByRole('button', { name: '詳しく表示' });
  const toCompact = page.getByRole('button', { name: '1 行にする' });
  if (want === 'expanded' && (await toExpanded.count()) > 0) await toExpanded.click();
  if (want === 'compact' && (await toCompact.count()) > 0) await toCompact.click();
  await expect(want === 'expanded' ? toCompact : toExpanded).toBeVisible();
}

const box = async (page: Page, sel: string) => {
  const b = await page.locator(sel).first().boundingBox();
  if (!b) throw new Error(sel);
  return b;
};

test.describe('CRM 架電画面: 案件の概要の見出し', () => {
  test.beforeEach(async ({ page }) => { await login(page); });

  for (const narrow of [false, true]) {
    const label = narrow ? '中央の列を狭めた配置' : '既定の配置';
    const tag = narrow ? 'narrow' : 'default';

    test(`詳しく表示: 社名が 1 文字ずつ縦に並ばず、概要が内容の高さに収まる (${label})`, async ({ page }) => {
      await open(page, narrow);
      await setDensity(page, 'expanded');
      const h2 = await box(page, '.wd-s-name h2');
      const top = await box(page, '.wd-top');
      const center = await box(page, '[data-testid="dock-col-1"]');
      console.log(`measure expanded ${tag} center=${String(Math.round(center.width))} name=${String(Math.round(h2.width))}x${String(Math.round(h2.height))} summary=${String(Math.round(top.height))}`);
      if (SHOT) await page.screenshot({ path: `${SHOT}/expanded-${tag}.png` });
      expect(h2.width).toBeGreaterThanOrEqual(150);
      expect(h2.height).toBeLessThanOrEqual(60);
      // 概要は幅で並べ替わる (crm_summary_responsive.spec.ts)。既定の配置 (約 430px) は 2 列で 275px 以内、380px 台は縦積みで 330px 以内
      expect(top.height).toBeLessThanOrEqual(narrow ? 330 : 275);
      // 見出しの中の操作が列からはみ出さない
      const sel = await box(page, '.wr-stage-select');
      expect(sel.x + sel.width).toBeLessThanOrEqual(center.x + center.width + 1);
    });

    test(`1 行にする: 社名は 1 行で、長いときは省略して title に全文 (${label})`, async ({ page }) => {
      await open(page, narrow);
      await setDensity(page, 'compact');
      const h2 = await box(page, '.wd-line-company');
      const top = await box(page, '.wd-top');
      console.log(`measure compact ${tag} name=${String(Math.round(h2.width))}x${String(Math.round(h2.height))} summary=${String(Math.round(top.height))}`);
      if (SHOT) await page.screenshot({ path: `${SHOT}/compact-${tag}.png` });
      expect(h2.width).toBeGreaterThanOrEqual(150);
      expect(h2.height).toBeLessThanOrEqual(30);
      await expect(page.locator('.wd-line-company')).toHaveAttribute('title', /.+/u);
    });
  }
});
