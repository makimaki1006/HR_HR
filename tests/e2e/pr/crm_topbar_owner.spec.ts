import { expect, test } from '@playwright/test';
import { login } from './helpers/login';

/**
 * 架電画面の上のバーは、狭い窓でも文字を 1 字ずつ折り返さない (タイトル「架電」は 1 行)。
 * ページが横にスクロールしない。所有者の一覧は、選んだらすぐ閉じる。
 */
for (const size of [{ width: 760, height: 700 }, { width: 390, height: 800 }]) {
  test(`topbar at ${size.width}px: title on one line, no horizontal scroll, owner list closes`, async ({ page }) => {
    await page.setViewportSize(size);
    await login(page);
    await page.goto('/app/crm?mode=fixture&pipeline=fx-sample');
    await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('架空サンプル');

    const title = page.getByRole('heading', { name: '架電', level: 1 });
    const box = await title.boundingBox();
    expect(box?.height ?? 999).toBeLessThanOrEqual(28);
    expect(await title.evaluate(el => getComputedStyle(el).whiteSpace)).toBe('nowrap');
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    // 操作ボタンは全部画面内に残る
    for (const name of ['元の配置に戻す', '実データ', '架空サンプル']) {
      await expect(page.getByRole('button', { name })).toBeVisible();
    }

    const trigger = page.getByLabel('所有者', { exact: true });
    await trigger.selectOption('pick');
    const list = page.getByLabel('所有者を選ぶ');
    await expect(list).toBeVisible();
    await list.selectOption('9002');
    await expect(list).toHaveCount(0);
    await expect(trigger).toBeFocused();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  });
}
