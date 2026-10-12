import { expect, test } from '@playwright/test';
import { login } from './helpers/login';

/**
 * 架電画面に上の濃いバーは無い。狭い窓でも絞り込みバーの「…」が画面内に残り、ページが横にスクロールしない。
 * 「…」メニューから元の配置に戻す・データの切り替えができる。所有者の一覧は、選んだらすぐ閉じる。
 */
for (const size of [{ width: 760, height: 700 }, { width: 390, height: 800 }]) {
  test(`filter bar at ${size.width}px: no top bar, 「…」 menu in view, no horizontal scroll, owner list closes`, async ({ page }) => {
    await page.setViewportSize(size);
    await login(page);
    await page.goto('/app/crm?mode=fixture&pipeline=fx-sample');
    await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('架空サンプル');

    await expect(page.getByRole('heading', { name: '架電', level: 1 })).toHaveCount(0);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    // 「…」は画面内に残り、開くと項目が画面内に出る
    const more = page.getByRole('button', { name: 'その他の操作' });
    await expect(more).toBeVisible();
    await more.click();
    for (const name of ['元の配置に戻す', '実データに戻す']) {
      const item = page.getByRole('menuitem', { name });
      await expect(item).toBeVisible();
      const b = await item.boundingBox();
      expect((b?.x ?? -1)).toBeGreaterThanOrEqual(0);
      expect((b?.x ?? 0) + (b?.width ?? 0)).toBeLessThanOrEqual(size.width);
    }
    await page.keyboard.press('Escape');
    await expect(page.getByRole('menu')).toHaveCount(0);
    await expect(more).toBeFocused();

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
