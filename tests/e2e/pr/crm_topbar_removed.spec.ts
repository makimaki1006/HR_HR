import { openQueueToolbar } from './helpers/crm_toolbar';
import { expect, test } from '@playwright/test';
import { login } from './helpers/login';

/**
 * 架電画面に上の濃いバー (HR_HR | 架電 / 実データの表示 / Zoom / 切り替え) は無い。
 * 架空サンプルのときだけ、上に帯を出す。切り替えと元の配置に戻すは、絞り込みバー右端の「…」から行う。
 */
test.describe('CRM 架電画面: 上のバーを無くした', () => {
  test.beforeEach(async ({ page }) => { await login(page); });

  test('架空サンプルの帯と「…」メニュー: 帯の文言・実データに戻す・メニューの 2 項目', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto('/app/crm?mode=fixture&pipeline=fx-sample');await openQueueToolbar(page);
    const banner = page.getByRole('status', { name: 'データの種類' });
    await expect(banner).toContainText('架空サンプルを表示中です。表示内容はすべて架空で、HubSpot には接続しません。');
    await expect(page.getByRole('heading', { name: '架電', level: 1 })).toHaveCount(0);
    await expect(page.getByTestId('zoom-toggle')).toHaveCount(0);

    await page.getByRole('button', { name: 'その他の操作' }).click();
    await expect(page.getByRole('menuitem')).toHaveText(['元の配置に戻す', '実データに戻す']);
    await page.keyboard.press('Escape');
    await expect(page.getByRole('menu')).toHaveCount(0);

    await banner.getByRole('button', { name: '実データに戻す' }).click();
    await expect(page.getByTestId('fixture-banner')).toHaveCount(0);
    await expect(page).not.toHaveURL(/mode=fixture/);
    await page.getByRole('button', { name: 'その他の操作' }).click();
    await expect(page.getByRole('menuitem')).toHaveText(['元の配置に戻す', '架空サンプルを表示']);
  });
});
