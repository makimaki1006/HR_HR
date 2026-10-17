import { expect, test } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { login } from './helpers/login';

test('自分の項目: またはのまとまり、専用の絞り込み、パイプライン別ステージを保存・復元する', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await login(page);
  await page.goto('/app/crm?mode=fixture&metrics=custom');
  await page.getByRole('button', { name: '⚙ 項目' }).click();
  const panel = page.getByRole('dialog', { name: '帯に出す項目' });
  await panel.getByTestId('metric-setting-c-absent').getByRole('button', { name: '直す' }).click();
  const one = panel.getByRole('region', { name: 'まとまり 1', exact: true });
  await expect(one).toContainText('最終架電日 今日');
  await expect(one).toContainText('不在');
  await panel.getByRole('button', { name: '＋ 「または」の条件を追加' }).click();
  const two = panel.getByRole('region', { name: 'まとまり 2', exact: true });
  await two.getByRole('combobox', { name: '項目を検索' }).click();
  const special = two.getByRole('group', { name: '取引の絞り込み' });
  await expect(special.getByRole('option')).toHaveText(['パイプライン', '取引ステージ']);
  await special.getByRole('option', { name: 'パイプライン', exact: true }).click();
  await two.getByRole('checkbox', { name: 'リクロジ受注管理_アポ前', exact: true }).check();
  await two.getByRole('button', { name: 'この条件を追加' }).click();
  await expect(two).toContainText('パイプライン いずれかに一致 リクロジ受注管理_アポ前');
  const shots = process.env.SHOTS;
  if (shots) {
    await mkdir(shots, { recursive: true });
    await panel.locator('.tm-set-editor').scrollIntoViewIfNeeded();
    await page.screenshot({ path: `${shots}/metric-two-groups.png` });
  }
  await two.getByRole('button', { name: '＋ 条件を追加' }).click();
  await two.getByRole('combobox', { name: '項目を検索' }).click();
  await two.getByRole('option', { name: '取引ステージ', exact: true }).click();
  await expect(two.getByRole('heading')).toHaveText(['リクロジ受注管理_アポ前']);
  const stage = two.getByRole('checkbox').first();
  // 別パイプラインの「不在」は候補に混ざらない。
  await expect(two.getByRole('checkbox', { name: '不在', exact: true })).toHaveCount(0);
  if (shots) {
    await two.getByRole('heading').scrollIntoViewIfNeeded();
    await page.screenshot({ path: `${shots}/metric-stage-pipelines.png` });
  }
  await stage.check();
  await two.getByRole('button', { name: 'この条件を追加' }).click();
  await panel.getByRole('button', { name: 'この項目を決める' }).click();
  await panel.getByRole('button', { name: '保存', exact: true }).click();
  await expect(panel.getByRole('status')).toHaveText('保存しました');
  await panel.getByRole('button', { name: '閉じる', exact: true }).click();
  await page.getByRole('button', { name: '⚙ 項目' }).click();
  await panel.getByTestId('metric-setting-c-absent').getByRole('button', { name: '直す' }).click();
  await expect(panel.getByText('または', { exact: true })).toBeVisible();
  await expect(panel.getByRole('region', { name: 'まとまり 2', exact: true })).toContainText('架空ステージ 1');
});
