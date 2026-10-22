import { expect, test } from '@playwright/test';
import { login } from './helpers/login';
import { openOtherAnalysis } from '../job-copy-navigation';

test.describe('応募理由の変更ごとの表示', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.route('**/api/job-copy/**', route => route.fulfill({ json: {} }));
    await login(page);
  });

  test('変更ごとに前後の応募と応募理由の件数を並べ、期間を切り替えられる', async ({ page }) => {
    await page.goto('/app/job-copy?demo=1&job=demo-job-001');
    await openOtherAnalysis(page, '応募理由');
    const dialog = page.getByRole('dialog', { name: 'ほかの分析', exact: true });
    await dialog.getByRole('button', { name: '変更ごと', exact: true }).click();
    await expect(page).toHaveURL(/reasonView=changes/);
    await expect(dialog.getByText('応募理由の記録はまだ少なく、1 つの求人・期間の件数だけで変更の効果は判断できません。')).toBeVisible();
    await expect(dialog.getByText('応募日も作成日も分からない記録 1件は、どの期間にも入れていません。')).toBeVisible();

    const blocks = dialog.getByTestId('change-reason-block');
    await expect(blocks).toHaveCount(2);
    const latest = blocks.nth(0);
    await expect(latest.getByRole('heading', { level: 3 })).toHaveText('9/25 · 給与・本文・画像');
    const latestBefore = latest.getByRole('group', { name: /^変更前/ });
    const latestAfter = latest.getByRole('group', { name: /^変更後/ });
    await expect(latestBefore).toContainText('9/15〜9/24（10日間）');
    await expect(latestBefore).toContainText('前の変更（9/15）までの10日で区切っています');
    await expect(latestBefore).toContainText('応募 8件');
    await expect(latestBefore).toContainText('応募理由の記録 5件');
    await expect(latestAfter).toContainText('応募 3件');
    await expect(latestAfter).toContainText('この期間に応募理由の記録はありません');

    const first = blocks.nth(1);
    await expect(first.getByRole('heading', { level: 3 })).toHaveText('9/15 · 給与・本文・画像');
    const firstBefore = first.getByRole('group', { name: /^変更前/ });
    await expect(firstBefore).toContainText('9/01〜9/14（14日間）');
    await expect(firstBefore).toContainText('応募 7件');
    await expect(firstBefore).toContainText('応募理由の記録 3件');
    await expect(firstBefore).toContainText('掲載の記録の始まり（9/01）からの14日で区切っています');
    await expect(dialog.getByRole('region', { name: '応募理由の確認', exact: true })).not.toContainText('%');

    await dialog.getByRole('button', { name: '前後 2週', exact: true }).click();
    await expect(dialog.getByRole('button', { name: '前後 2週', exact: true })).toHaveAttribute('aria-pressed', 'true');
    await expect(firstBefore).toContainText('9/01〜9/14（14日間）');
    await expect(firstBefore).not.toContainText('区切っています');

    await dialog.getByRole('button', { name: '要因メモを開く', exact: true }).click();
    await expect(page).toHaveURL(/feature=factors/);
  });

  test('AirWork の求人は変わった日を幅のまま出し、間の日は数えない', async ({ page }) => {
    await page.goto('/app/job-copy?demo=1&job=demo-job-004&analysis=1&feature=reasons&reasonView=changes');
    const dialog = page.getByRole('dialog', { name: 'ほかの分析', exact: true });
    const block = dialog.getByTestId('change-reason-block');
    await expect(block).toHaveCount(1);
    await expect(block.getByRole('heading', { level: 3 })).toHaveText('9/10〜9/28 の間 · 給与・本文・画像');
    await expect(block.getByRole('group', { name: /^変更前/ })).toContainText('この本文が出ていたと確かな日がないため、数えていません');
    const after = block.getByRole('group', { name: /^変更後/ });
    await expect(after).toContainText('9/29〜10/03（5日間）');
    await expect(after).toContainText('応募 2件');
    await expect(after).toContainText('応募理由の記録 2件');
  });
});
