import { expect, test } from '@playwright/test';
import { login } from './helpers/login';

/**
 * 架電画面 (/app/crm?mode=fixture) の活動ログ: メールを HubSpot のように読みやすく出す。
 * 架空サンプルのメール (HTML の本文・署名・引用返信つき) で、件名・差出人・全文表示・引用の開閉を確かめる。
 * HubSpot には接続しない。
 */
test.describe('CRM 活動ログ: メールのカード', () => {
  test.beforeEach(async ({ page }) => { await login(page); });

  test('件名・送信・差出人 → 宛先が見え、全文表示で本文が、引用は別の開閉で出る', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto('/app/crm?mode=fixture');
    const list = page.getByRole('list', { name: '架電キュー' });
    await list.locator('li.cq-row button.cq-row-button').first().click();
    await page.getByRole('tab', { name: '活動ログ' }).click();
    const log = page.getByRole('region', { name: '活動ログ' });

    const card = log.locator('[data-testid="activity-card"][data-kind="email"]').first();
    await expect(card.getByTestId('activity-title')).toHaveText('ご挨拶(架空)');
    await expect(card.getByTestId('activity-direction')).toHaveText('送信');
    await expect(card.getByTestId('activity-addr')).toContainText('架空 太郎 <taro@example.invalid> → contact@example.invalid');
    await expect(card).toContainText('添付 1 件');
    // 折りたたみ中は下見だけ。本文の箱も引用も出ていない
    await expect(card.getByTestId('activity-preview')).toContainText('ダミー建設 ご担当者様');
    await expect(card.getByTestId('activity-body')).toHaveCount(0);

    await card.getByRole('button', { name: '全文を表示' }).click();
    const body = card.getByTestId('activity-body');
    await expect(body).toContainText('資料を送付します');
    await expect(body).toContainText('株式会社サンプル');
    await expect(body.locator('b')).toHaveText('資料');
    // 引用 (以前のやりとり) は本文に入らず、別の開閉で出る
    await expect(body).not.toContainText('先日の件、資料をお願いします');
    await expect(card.getByTestId('activity-quoted')).toHaveCount(0);
    await card.getByRole('button', { name: '以前のやりとりを表示' }).click();
    await expect(card.getByTestId('activity-quoted')).toContainText('先日の件、資料をお願いします');

    // 絞り込みのチップは今までどおり動く
    await log.getByRole('button', { name: 'メモ' }).click();
    await expect(log.locator('[data-testid="activity-card"][data-kind="email"]')).toHaveCount(0);
    await expect(log.locator('[data-testid="activity-card"][data-kind="note"]')).toHaveCount(1);
  });
});
