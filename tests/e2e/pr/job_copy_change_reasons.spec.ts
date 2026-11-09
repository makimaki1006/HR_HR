import { expect, test, type Locator } from '@playwright/test';
import { login } from './helpers/login';
import { openOtherAnalysis } from '../job-copy-navigation';

const motive = 'oubodouki_1';
const priorities = ['kinmujoukennoyuusenjuni_1', 'kinmujoukennoyuusenjuni_2', 'kinmujoukennoyuusenjuni_3'];
const options = Array.from({ length: 13 }, (_, index) => ({ value: `synthetic-${String(index + 1)}`, label: index === 0 ? '給与が高い' : index === 12 ? 'これまでの経験や資格を生かして働ける' : `架空の選択肢${String(index + 1)}`, display_order: index }));
const selectionOptions = Object.fromEntries([motive, ...priorities].map(property => [property, options]));
const selections = (motives: number[], first: number | null, second: number | null, third: number | null) => Object.fromEntries([
  [motive, motives.map(value => `synthetic-${String(value)}`)],
  ...priorities.map((property, index) => [property, [first, second, third][index] == null ? [] : [`synthetic-${String([first, second, third][index])}`]]),
]);
const heard = [
  selections([1, 2], 2, 3, null), selections([2], 1, null, 3), selections([1], 1, 2, null),
  selections([3], 3, null, 1), selections([2, 3], 2, 1, null),
];
const afterChoices = [selections([1], 3, 2, null), selections([2, 3], 1, null, 3)];
function applicants(id: string) {
  const dates = id === '301'
    ? ['2026-09-02', '2026-09-04', '2026-09-04', '2026-09-07', '2026-09-10', '2026-09-10', '2026-09-13', '2026-09-16', '2026-09-16', '2026-09-16', '2026-09-18', '2026-09-18', '2026-09-21', '2026-09-21', '2026-09-24', '2026-09-27', '2026-09-30', '2026-10-02']
    : ['2026-09-15', '2026-09-28', '2026-10-02'];
  const missing = id === '301' ? 10 : 3;
  const choices = id === '301'
    ? dates.map((_, index) => index >= 10 && index <= 14 ? heard[index - 10] : index === 15 ? afterChoices[0] : index === 17 ? afterChoices[1] : selections([], null, null, null))
    : [selections([], null, null, null), ...afterChoices];
  const applications = [...dates, ...Array<string | null>(missing).fill(null)].map((date, index) => ({
    key: index.toString(16).padStart(64, '0'), date, gender: null, age: null, prefecture: null, municipality: null,
    selections: index < dates.length ? choices[index] : index === dates.length ? selections([3], null, null, null) : selections([], null, null, null),
  }));
  const byDate: Record<string, number> = {};
  for (const date of dates) byDate[date] = (byDate[date] ?? 0) + 1;
  return { listing_id: id, total: applications.length, missing_date: missing, by_date: byDate, fetched_at: '2026-10-05T00:00:00Z', applications, selection_options: selectionOptions };
}
function listing(id: string) {
  return { id, media: id === '301' ? 'hrh' : 'airwork', media_job_id: `synthetic-${id}`, account_id: null, title: '架空の配送スタッフ', prefecture: '大分県', municipality: '大分市', category: null, publication_status: '掲載中', last_csv_detected_at: null, application_count: applicants(id).total, indeed_title_state: 'none' };
}
function versions(id: string) {
  const days = id === '301' ? ['2026-09-01', '2026-09-15', '2026-09-25'] : ['2026-09-10', '2026-09-28'];
  return { listing: listing(id), history_counts: {}, history_may_be_incomplete: false, versions: days.map((day, index) => ({
    written_at: `${day}T03:00:00Z`, body: `仕事内容：架空の配送作業${String(index + 1)}です。\n給与：月給${String(230000 + index * 10000)}円`,
    image_urls: [`data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><text y="10">${String(index)}</text></svg>`)}`],
  })) };
}
async function expectSelectionTables(window: Locator, total: number, denominators: number[], counts: number[][]) {
  const motivation = window.getByRole('table', { name: '応募動機の選択件数', exact: true });
  const priority = window.getByRole('table', { name: '勤務条件の優先順位の選択件数', exact: true });
  await expect(motivation.getByRole('columnheader')).toHaveText(['選択肢', '件数']);
  await expect(priority.getByRole('columnheader')).toHaveText(['選択肢', '①', '②', '③']);
  await expect(window.getByText(`応募動機を聞けた応募 ${String(denominators[0])}件／この求人の全応募 ${String(total)}件`, { exact: true })).toBeVisible();
  await expect(window.getByText(`①を聞けた応募 ${String(denominators[1])}件・②${String(denominators[2])}件・③${String(denominators[3])}件／この求人の全応募 ${String(total)}件`, { exact: true })).toBeVisible();
  for (const table of [motivation, priority]) {
    await expect(table.getByRole('rowheader')).toHaveText(options.map(option => option.label));
    await expect(table.getByRole('row')).toHaveCount(14);
    // 前後を並べる幅でも選択肢名の幅を確保し、件数はすべて右寄せにする。
    expect(await table.getByRole('rowheader').first().evaluate(element => element.getBoundingClientRect().width)).toBeGreaterThanOrEqual(160);
    expect(await table.locator('tbody td').evaluateAll(cells => cells.every(cell => getComputedStyle(cell).textAlign === 'right'))).toBe(true);
  }
  await expect(motivation.getByRole('cell')).toHaveText(Array.from({ length: 13 }, (_, option) => `${String(counts[0]?.[option] ?? 0)}件`));
  await expect(priority.getByRole('cell')).toHaveText(Array.from({ length: 13 }, (_, option) => [1, 2, 3].map(rank => `${String(counts[rank]?.[option] ?? 0)}件`)).flat());
}

test.describe('応募理由の変更ごとの表示', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    // 固定デモは応募APIを読まないため、求人履歴と応募を同じ偽APIから読み込ませる。
    await page.route('**/api/job-copy/**', route => {
      const path = new URL(route.request().url()).pathname;
      const id = path.split('/')[4] ?? '';
      if (path.endsWith('/versions')) return route.fulfill({ json: versions(id) });
      if (path.endsWith('/applicants')) return route.fulfill({ json: applicants(id) });
      if (path.endsWith('/notes')) return route.fulfill({ json: { notes: {} } });
      if (path.endsWith('/companies')) return route.fulfill({ json: { companies: [] } });
      return route.fulfill({ json: {} });
    });
    await login(page);
  });

  test('変更ごとに前後の応募と選択式の件数・母数を並べ、期間を切り替えられる', async ({ page }) => {
    await page.goto('/app/job-copy?listing=301');
    await openOtherAnalysis(page, '応募理由');
    const dialog = page.getByRole('dialog', { name: 'ほかの分析', exact: true });
    await dialog.getByRole('button', { name: '変更ごと', exact: true }).click();
    await expect(page).toHaveURL(/reasonView=changes/);
    await expect(dialog.getByText('応募理由の記録はまだ少なく、1 つの求人・期間の件数だけで変更の効果は判断できません。')).toBeVisible();
    await expect(dialog.getByText('応募日も作成日も分からない記録 1件は、どの期間にも入れていません。')).toBeVisible();
    await expect(dialog.getByLabel('変更ごとの応募理由').getByText('選択式の記録は 2026/09/18 の応募からです')).toBeVisible();

    const blocks = dialog.getByTestId('change-reason-block');
    await expect(blocks).toHaveCount(2);
    const latest = blocks.nth(0);
    await expect(latest.getByRole('heading', { level: 3, name: /給与・本文・画像$/ })).toHaveText('9/15〜9/25 の間 · 給与・本文・画像');
    const latestBefore = latest.getByRole('group', { name: /^変更前/ });
    const latestAfter = latest.getByRole('group', { name: /^変更後/ });
    await expect(latestBefore).toContainText('9/15〜9/24（10日間）');
    await expect(latestBefore).toContainText('前の変更（9/15）までの10日で区切っています');
    await expect(latestBefore).toContainText('応募 8件');
    // 9/18の2件・9/21の2件・9/24の1件を集計。優先順位②は3件、③は2件のみ回答している。
    await expectSelectionTables(latestBefore, 28, [5, 5, 3, 2], [[2, 3, 2], [2, 2, 1], [1, 1, 1], [1, 0, 1]]);
    await expect(latestAfter).toContainText('応募 3件');
    // 変更後3件のうち9/27と10/02に選択式の記録があり、9/30は未回答。
    await expectSelectionTables(latestAfter, 28, [2, 2, 1, 1], [[1, 1, 1], [1, 0, 1], [0, 1], [0, 0, 1]]);

    const first = blocks.nth(1);
    await expect(first.getByRole('heading', { level: 3, name: /給与・本文・画像$/ })).toHaveText('9/01〜9/15 の間 · 給与・本文・画像');
    const firstBefore = first.getByRole('group', { name: /^変更前/ });
    await expect(firstBefore).toContainText('9/01〜9/14（14日間）');
    await expect(firstBefore).toContainText('応募 7件');
    // 9/18より前の7件には選択式の値を入れていない。文章からの推定で補わない。
    await expectSelectionTables(firstBefore, 28, [0, 0, 0, 0], [[], [], [], []]);
    await expect(firstBefore).toContainText('掲載の記録の始まり（9/01）からの14日で区切っています');
    await expect(dialog.getByRole('region', { name: '応募理由の確認', exact: true })).not.toContainText('%');

    await dialog.getByRole('button', { name: '前後 2週', exact: true }).click();
    await expect(dialog.getByRole('button', { name: '前後 2週', exact: true })).toHaveAttribute('aria-pressed', 'true');
    await expect(firstBefore).toContainText('9/01〜9/14（14日間）');
    await expect(firstBefore).not.toContainText('区切っています');
    await dialog.getByRole('button', { name: '要因メモを開く', exact: true }).click();
    await expect(page).toHaveURL(/feature=factors/);
  });

  test('AirWorkの記録日の間は前の本文に入れ、変更当日は変更後に含める', async ({ page }) => {
    await page.goto('/app/job-copy?listing=304&analysis=1&feature=reasons&reasonView=changes');
    const dialog = page.getByRole('dialog', { name: 'ほかの分析', exact: true });
    const block = dialog.getByTestId('change-reason-block');
    await expect(block).toHaveCount(1);
    await expect(block.getByRole('heading', { level: 3, name: /給与・本文・画像$/ })).toHaveText('9/10〜9/28 の間 · 給与・本文・画像');
    const before = block.getByRole('group', { name: /^変更前/ });
    await expect(before).toContainText('9/10〜9/27（18日間）');
    await expect(before).toContainText('応募 1件');
    await expectSelectionTables(before, 6, [0, 0, 0, 0], [[], [], [], []]);
    const after = block.getByRole('group', { name: /^変更後/ });
    await expect(after).toContainText('9/28〜10/05（8日間）');
    // 9/15は前の本文、変更当日の9/28と10/02の2件は後の本文に入る。
    await expect(after).toContainText('応募 2件');
    await expectSelectionTables(after, 6, [2, 2, 1, 1], [[1, 1, 1], [1, 0, 1], [0, 1], [0, 0, 1]]);
  });
});
