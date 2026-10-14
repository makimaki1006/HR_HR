import { expect, Page, test } from '@playwright/test';
import { login } from './helpers/login';

/**
 * 架電画面 (/app/crm?mode=fixture) の案件の見出し: 2 行に固定。
 * 1 行目 = 会社・担当者 + 右端に「HubSpotで開く」「詳しく表示」、2 行目 = 電話番号・発信。
 * 会社名の長さで番号・発信・HubSpotで開く の位置が動かない。前回の活動は見出しの下に 1 行。
 * SCREENSHOT_DIR を指定すると、確認用の画像を残す。
 */

const SCREEN_URL = '/app/crm?mode=fixture&pipeline=fx-sample';
const SHORT = '架空ベーカリー';
const LONG = '架空サーモスタット工業株式会社';
const SHOT = process.env.SCREENSHOT_DIR;

async function pick(page: Page, name: string): Promise<void> {
  await page.getByRole('list', { name: '架電キュー' }).getByRole('button', { name: new RegExp(name, 'u') }).first().click();
  await expect(page.getByRole('article', { name: '架電先の詳細' }).locator('h2').first()).toHaveText(name);
}

const box = async (page: Page, sel: string) => {
  const b = await page.locator(sel).first().boundingBox();
  if (!b) throw new Error(sel);
  return b;
};

async function measure(page: Page) {
  return {
    phone: await box(page, '[data-testid="head-line-2"] .cq-phone'),
    dial: await box(page, '[data-testid="head-line-2"] .wd-dial'),
    link: await box(page, '[data-testid="head-line-1"] a'),
    toggle: await box(page, '[data-testid="head-line-1"] .wd-density'),
  };
}

test.describe('CRM 架電画面: 案件の見出し (2 行)', () => {
  test.beforeEach(async ({ page }) => { await login(page); });

  for (const [w, h] of [[1512, 742], [1366, 768]] as const) {
    test(`会社名の長さで番号・発信・HubSpotで開くの位置が動かない (${String(w)}x${String(h)})`, async ({ page }) => {
      await page.setViewportSize({ width: w, height: h });
      await page.goto(SCREEN_URL);
      await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('架空サンプル');
      // 左の欄の右端に縦のつまみ (取引一覧 / プロパティ) がある
      await expect(page.getByRole('tab', { name: '取引一覧' })).toBeVisible();
      await pick(page, SHORT);
      const short = await measure(page);
      if (SHOT) await page.screenshot({ path: `${SHOT}/${w === 1512 ? 'g2-short' : 'g2-1366'}.png` });
      // 2 行: 番号は 1 行目より下、発信ボタンと同じ行
      expect(short.phone.y).toBeGreaterThan(short.link.y + short.link.height - 2);
      expect(Math.abs(short.dial.y + short.dial.height / 2 - (short.phone.y + short.phone.height / 2))).toBeLessThan(14);
      await expect(page.getByTestId('last-activity')).toHaveText('前回 10/03 通話・受付で不在。来週再架電。');
      // 押せる行: 押すと活動ログのタブが前に出る
      await page.getByRole('tab', { name: '架電結果の入力' }).click();
      await page.getByTestId('last-activity').getByRole('button').click();
      await expect(page.getByRole('tab', { name: '活動ログ' })).toHaveAttribute('aria-selected', 'true');
      await pick(page, LONG);
      const long = await measure(page);
      if (SHOT && w === 1512) await page.screenshot({ path: `${SHOT}/g2-long.png` });
      expect(Math.abs(long.phone.x - short.phone.x)).toBeLessThan(1);
      expect(Math.abs(long.phone.y - short.phone.y)).toBeLessThan(1);
      expect(Math.abs(long.dial.x - short.dial.x)).toBeLessThan(1);
      expect(Math.abs(long.link.x - short.link.x)).toBeLessThan(1);
      expect(Math.abs(long.link.y - short.link.y)).toBeLessThan(1);
      expect(Math.abs(long.toggle.x - short.toggle.x)).toBeLessThan(1);
      // 長い会社名は省略表示で、title に全文
      await expect(page.locator('.wd-line-company')).toHaveAttribute('title', LONG);
      // 1 行目の右端に「HubSpotで開く」「詳しく表示」が収まる
      const line1 = await box(page, '[data-testid="head-line-1"]');
      expect(long.toggle.x + long.toggle.width).toBeLessThanOrEqual(line1.x + line1.width + 1);
      // 詳しく表示でも崩れない: 見出しは残り、前回の活動の行も出る
      await page.getByRole('button', { name: '詳しく表示' }).click();
      await expect(page.getByRole('region', { name: '架ける番号' })).toBeVisible();
      await expect(page.getByTestId('last-activity')).toBeVisible();
    });
  }
});
