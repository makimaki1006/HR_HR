import { expect, test } from '@playwright/test';
import { boardFixture, boardRows, firstRows } from '../../../frontend/src/screens/job-copy/dealBoard.testFixtures';
import { demoHome } from '../../../frontend/src/screens/job-copy/demoHome';
import { demoOwnerTodo } from '../../../frontend/src/screens/job-copy/demoOwnerTodo';
import { login } from './helpers/login';

test('案件一覧の印・日付・並べ替え・絞り込み・20件送りと戻る', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  const requests: string[] = [];
  await page.route('**/api/job-copy/deals?*', route => {
    requests.push(route.request().url());
    const owner = new URL(route.request().url()).searchParams.get('owner') ?? '11';
    return route.fulfill({ json: { ...boardFixture, rows: owner === '22' ? [] : boardRows, work: { ...demoOwnerTodo, owner } } });
  });
  // 「いい」の印: いい案件の集計にある案件（100 と 105）だけを出す
  await page.route('**/api/job-copy/consultant-good?*', route => route.fulfill({ json: { status: 'ready', data_day: '2026-10-15', previous_day: null, comparison: 'unavailable', rows: ['100', '105'].map(id => ({ deal_id: id, name: `架空の採用支援${String(Number(id) - 99).padStart(2, '0')}`, company: null, consultant: '合成担当A', flags: [], risk_flags: [], good_flags: [{ key: 'applications', label: '応募 3 → 7', new: false, since: null }], days_left: null, days_since_contact: null, mtg_last: null, actionyotei: null, actionnaiyou: null })) } }));
  await page.route('**/api/job-copy/home*', route => route.fulfill({ json: { ...demoHome, work: demoOwnerTodo } }));
  await page.route('**/api/job-copy/recent-changes*', route => route.fulfill({ json: { status: 'ready', refresh_failed: false, report: { count: 0, listings: [] } } }));
  await page.route('**/api/job-copy/moc', route => route.fulfill({ json: { schemaVersion: 1, capture_bundle: { schemaVersion: 1, jobs: [] }, results: [] } }));
  await page.route('**/api/job-copy/listings?*', route => route.fulfill({ json: { status: 'ready', total: 0, listings: [], deals: [], titles: [] } }));
  await login(page); await page.goto('/app/job-copy?owner=11&owner_field=consultant');
  await page.getByRole('button', { name: '案件の一覧', exact: true }).click();
  await expect(page).toHaveURL(/view=deals/);
  await expect(page.getByRole('heading', { name: '案件の一覧' })).toBeVisible();
  const table = page.locator('.db-table');
  await expect(table.locator('tbody > tr')).toHaveCount(20);
  await expect(table.locator('tbody > tr').first()).toContainText('架空の採用支援01');
  await expect(page.getByText('23件・1 / 2ページ')).toBeVisible();
  await expect(page.getByText(/2026\/10\/15 時点（前日の集計）/)).toBeVisible();
  await expect(table.locator('.nt-strong')).toHaveText('MTGが60日以上空いている');
  await expect(page.getByRole('checkbox', { name: '終わった案件も出す' })).not.toBeChecked();
  await expect(page.getByRole('combobox', { name: '並べ替え' })).toHaveCount(0);
  await expect(page.getByText('並び: 要対応の印が多い順', { exact: true })).toBeVisible();
  await expect(table.locator('thead th[aria-sort="descending"]')).toHaveCount(0);
  const crumbs = page.getByRole('navigation', { name: '現在地' });
  await expect(crumbs).toContainText('案件管理案件の一覧');
  const controls = page.locator('.db-filters');
  const layout = await controls.locator('select, input[type=checkbox]').evaluateAll(elements => elements.map(el => ({ top: el.getBoundingClientRect().top, center: el.getBoundingClientRect().top + el.getBoundingClientRect().height / 2 })));
  expect(Math.max(...layout.map(box => box.center)) - Math.min(...layout.map(box => box.center))).toBeLessThan(3);
  const rowHeight = await table.locator('tbody > tr').first().evaluate(el => el.getBoundingClientRect().height);
  expect(rowHeight).toBeLessThan(60);
  expect(await page.locator('.db-board').evaluate(el => el.scrollWidth > el.clientWidth)).toBe(false);
  const dir = process.env.BOARD_SCREENSHOT_DIR;
  if (dir) await page.screenshot({ path: `${dir}/deals-1440.png`, fullPage: true });
  const marks = page.getByRole('group', { name: '要対応の印で絞る' });
  const nps = marks.getByRole('button', { name: '定期NPSが低い 1', exact: true });
  await nps.click();
  await expect(nps).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByText('1件・1 / 1ページ')).toBeVisible();
  await expect(table.locator('tbody > tr')).toHaveCount(1);
  await expect(marks.getByRole('button', { name: 'MTGが途絶えている 1', exact: true })).toBeVisible();
  if (dir) await page.screenshot({ path: `${dir}/deals-filtered.png`, fullPage: true });
  await nps.click();
  await expect(page.getByText('23件・1 / 2ページ')).toBeVisible();
  await marks.getByRole('button', { name: '定期NPSが低い 1', exact: true }).click();
  await marks.getByRole('button', { name: 'すべて 23', exact: true }).click();
  const zero = marks.getByRole('button', { name: '採用単価が高い 0', exact: true });
  await expect(zero).toHaveClass(/db-zero/);
  await zero.click();
  await expect(table).toContainText('条件に合う案件はありません');
  await marks.getByRole('button', { name: 'すべて 23', exact: true }).click();
  // 既定の列は 8 つ。採用・採用単価・金額・採用目標は「詳しい数字」を開くと出る
  await expect(table.locator('thead th')).toHaveText(['案件・顧客 ▲▼', '要対応の印 ▲▼', '応募（直近4週） ▲▼', '満了まで ▲▼', '最後の接触 ▲▼', '最後のMTG ▲▼', 'NPS ▲▼', '担当 ▲▼', '詳しい数字']);
  // 「いい」は印の先頭（すべての次）に並ぶ
  await expect(marks.getByRole('button').nth(1)).toHaveText(/^いい\s*2$/);
  const good = marks.getByRole('button', { name: 'いい 2', exact: true });
  await good.click();
  await expect(table.locator('tbody > tr')).toHaveCount(2);
  await expect(table.locator('tbody > tr').first()).toContainText('架空の採用支援01');
  await expect(page).toHaveURL(/deals_reason=good/);
  await good.click();
  await expect(page.getByText('23件・1 / 2ページ')).toBeVisible();
  await table.locator('tbody > tr').first().getByRole('button', { name: '架空の採用支援01の詳しい数字を開く' }).click();
  await expect(table.locator('.db-detail').getByRole('columnheader', { name: /定期NPS/ })).toBeVisible();
  await expect(table.locator('.db-detail tbody td').nth(3)).toHaveText('4');
  await expect(table.locator('.db-detail tbody td').nth(7)).toHaveText('¥50,000');
  await expect(table.locator('.db-detail tbody td').nth(8)).toHaveText('¥100,000');
  await expect(table.locator('.db-detail tbody td').last()).toContainText('2 / 10人');
  await table.getByRole('button', { name: '架空の採用支援01の詳しい数字を開く' }).click();
  await expect(table.locator('.db-detail')).toHaveCount(0);
  await table.getByRole('columnheader', { name: /応募（直近4週）/ }).getByRole('button').click();
  await expect(table.getByRole('columnheader', { name: /応募（直近4週）/ })).toHaveAttribute('aria-sort', 'descending');
  await expect(table.locator('tbody > tr').first()).toContainText('架空の採用支援24');
  await table.getByRole('columnheader', { name: /応募（直近4週）/ }).getByRole('button').click();
  await expect(table.getByRole('columnheader', { name: /応募（直近4週）/ })).toHaveAttribute('aria-sort', 'ascending');
  await expect(table.locator('tbody > tr').first()).toContainText('架空の採用支援01');
  await table.getByRole('columnheader', { name: /応募（直近4週）/ }).getByRole('button').click();
  await page.getByRole('button', { name: '次の20件' }).click();
  await expect(page.getByText('23件・2 / 2ページ')).toBeVisible();
  await expect(table.locator('tbody > tr:not(.db-detail)')).toHaveCount(3);
  await page.getByLabel('終わった案件も出す').check();
  await expect(page.getByText('24件・1 / 2ページ')).toBeVisible();
  await page.getByRole('combobox', { name: 'パイプライン', exact: true }).selectOption('743961896');
  await expect(table.locator('tbody > tr')).toHaveCount(1);
  await expect(table).toContainText('記録なし（対象外）');
  await page.getByRole('combobox', { name: '担当者', exact: true }).selectOption('22');
  await expect(table).toContainText('条件に合う案件はありません');
  expect(requests.some(url => new URL(url).searchParams.get('owner') === '22')).toBe(true);
  await page.getByRole('combobox', { name: '担当者', exact: true }).selectOption('11');
  await page.getByRole('combobox', { name: 'パイプライン', exact: true }).selectOption('all');
  await page.getByRole('button', { name: '標準の並びに戻す' }).click();
  const overflow = await page.locator('.db-board').evaluate(el => el.scrollWidth > el.clientWidth);
  expect(overflow).toBe(false);

  await page.getByRole('button', { name: '架空の採用支援01', exact: true }).click();
  await expect(page).toHaveURL(/risk_deal=100/);
  const detail = page.getByRole('dialog');
  await expect(detail).toBeVisible();
  await expect(detail.getByRole('link', { name: '取引の画面を開く' })).toHaveAttribute('href', /[?&]deal=100(?:&|$)/);
  await page.goBack();
  await expect(page.getByRole('heading', { name: '案件の一覧' })).toBeVisible();
  await page.goBack();
  await expect(page.getByText('23件・1 / 2ページ')).toBeVisible();
  await page.goBack();
  await expect(page.getByRole('heading', { name: '今日', exact: true })).toBeVisible();
  await page.goForward();
  await expect(page.getByRole('heading', { name: '案件の一覧' })).toBeVisible();
  await page.getByRole('navigation', { name: '現在地' }).getByRole('button', { name: '案件管理', exact: true }).click();
  await expect(page.getByRole('heading', { name: '今日', exact: true })).toBeVisible();
});

test('シート未取得でも案件とHubSpotの値を表示する', async ({ page }) => {
  await page.route('**/api/job-copy/deals?*', route => route.fulfill({ json: { ...boardFixture, sheet_as_of: null, rows: [{ ...boardRows[0], mtg_status: 'unavailable', last_mtg: null, reasons: [{ kind: 'nps', text: '定期NPSが4以下' }] }] } }));
  await login(page); await page.goto('/app/job-copy?view=deals&owner=11');
  await expect(page.locator('.db-table')).toContainText('未取得');
  await expect(page.locator('.db-table .nt-reason')).toHaveText('定期NPSが4以下');
});


test('初回案件の立ち上がり・要約・URL・ホームのカード', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.route('**/api/job-copy/deals?*', route => route.fulfill({ json: { ...boardFixture, rows: firstRows } }));
  await page.route('**/api/job-copy/home*', route => route.fulfill({ json: { ...demoHome, work: { ...demoOwnerTodo, signals: firstRows.filter(row => row.first_contract && row.launch.status === 'below').map(row => ({ ...demoOwnerTodo.signals?.[0], id: row.id, name: row.name, reasons: row.reasons })) } } }));
  await page.route('**/api/job-copy/recent-changes*', route => route.fulfill({ json: { status: 'ready', refresh_failed: false, report: { count: 0, listings: [] } } }));
  await page.route('**/api/job-copy/moc', route => route.fulfill({ json: { schemaVersion: 1, capture_bundle: { schemaVersion: 1, jobs: [] }, results: [] } }));
  await page.route('**/api/job-copy/listings?*', route => route.fulfill({ json: { status: 'ready', total: 0, listings: [], deals: [], titles: [] } }));
  await login(page); await page.goto('/app/job-copy?view=deals&owner=11');
  await page.getByLabel('初回案件だけ', { exact: true }).check();
  await expect(page).toHaveURL(/deals_first=yes/);
  await expect(page.getByText('5件・1 / 1ページ', { exact: true })).toBeVisible();
  const table = page.locator('.db-table');
  await expect(table.locator('tbody > tr').first()).toContainText('架空の採用支援04');
  await expect(table).not.toContainText('架空の採用支援06');
  await expect(table.getByRole('columnheader', { name: '金額', exact: true })).toHaveCount(0);
  await expect(table.locator('tbody > tr').nth(1)).toContainText('9件目安より少ない（あと1件）');
  await expect(table.locator('tbody > tr').nth(1)).toContainText('開始から8日目');
  await expect(table.getByRole('img').nth(1)).toHaveAttribute('aria-label', '1週目 0件、2週目 2件、3週目 1件、4週目 0件、5週目 1件、6週目 0件、7週目 0件、8週目 5件');
  const summary = page.getByRole('group', { name: '初回案件の応募で絞る' });
  await expect(summary.getByRole('button')).toHaveText(['初回案件 5件：', '立ち上がり中 1', '目安どおり 1', '目安より少ない 2', '未取得 1']);
  expect(await page.locator('.db-board').evaluate(el => el.scrollWidth > el.clientWidth)).toBe(false);
  const dir = process.env.BOARD_SCREENSHOT_DIR;
  if (dir) await page.screenshot({ path: `${dir}/first-contracts-1440.png`, fullPage: true });
  await summary.getByRole('button', { name: '目安より少ない 2', exact: true }).click();
  await expect(page.getByText('2件・1 / 1ページ', { exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByText('2件・1 / 1ページ', { exact: true })).toBeVisible();
  await page.getByRole('group', { name: '要対応の印で絞る' }).getByRole('button', { name: '定期NPSが低い 1', exact: true }).click();
  await expect(page.getByText('1件・1 / 1ページ', { exact: true })).toBeVisible();
  await page.getByRole('combobox', { name: 'パイプライン', exact: true }).selectOption('743961896');
  await expect(table).toContainText('条件に合う案件はありません');
  await page.goto('/app/job-copy?owner=11');
  const homeCard = page.getByRole('button', { name: /初回で成果が出ていない/ });
  await expect(homeCard).toHaveText('初回で成果が出ていない 2件');
  await homeCard.click();
  await expect(page).toHaveURL(/view=deals/);
  await expect(page.getByLabel('初回案件だけ', { exact: true })).toBeChecked();
  await expect(page.getByText('2件・1 / 1ページ', { exact: true })).toBeVisible();
  if (dir) await page.screenshot({ path: `${dir}/first-from-home-1440.png`, fullPage: true });
  await page.getByRole('group', { name: '初回案件の応募で絞る' }).getByRole('button', { name: '未取得 1', exact: true }).click();
  await expect(table.locator('tbody > tr')).toContainText('未取得');
  await expect(table.locator('tbody > tr')).not.toContainText('0件');
});


test('応募を集計中のときは0件として表示しない', async ({ page }) => {
  await page.route('**/api/job-copy/deals?*', route => route.fulfill({ json: { ...boardFixture, applications_status: 'preparing', rows: firstRows.filter(row => row.launch.status === 'unavailable') } }));
  await login(page); await page.goto('/app/job-copy?view=deals&owner=11&deals_first=yes');
  const row = page.locator('.db-table tbody > tr');
  await expect(row).toContainText('集計中');
  await expect(row).not.toContainText('0件');
  await expect(row.getByRole('img')).toHaveCount(0);
});

test('求人と応募の前回集計をやることと案件一覧に残して日時を表示する', async ({ page }) => {
  const saved = { facts_built_at: '2026-10-15T15:10:00Z', facts_stale: true, refreshing: true };
  // 集計の時点から6時間未満の「今」に固定する（それ以上古いと文言が変わるため）。
  await page.clock.setFixedTime(new Date('2026-10-15T17:00:00Z'));
  await page.route('**/api/job-copy/home*', route => route.fulfill({ json: { ...demoHome, work: { ...demoOwnerTodo, ...saved } } }));
  await page.route('**/api/job-copy/deals?*', route => route.fulfill({ json: { ...boardFixture, ...saved } }));
  await page.route('**/api/job-copy/recent-changes*', route => route.fulfill({ json: { status: 'ready', refresh_failed: false, report: { count: 0, listings: [] } } }));
  await page.route('**/api/job-copy/moc', route => route.fulfill({ json: { schemaVersion: 1, capture_bundle: { schemaVersion: 1, jobs: [] }, results: [] } }));
  await login(page);
  await page.goto('/app/job-copy?owner=11&owner_field=consultant');
  await expect(page.getByText('2026/10/16 00:10 時点の集計です。新しい集計を作っています。')).toBeVisible();
  await expect(page.locator('.jh-todo-cards').getByRole('button', { name: /契約満了が近い/ }).locator('.jh-count')).toHaveText('1件');
  await page.getByRole('button', { name: '案件の一覧', exact: true }).click();
  await expect(page.getByText('2026/10/16 00:10 時点の集計です。新しい集計を作っています。')).toBeVisible();
  await expect(page.locator('.db-table tbody > tr')).toHaveCount(20);
  await expect(page.locator('.db-table tbody > tr').first()).toContainText('架空の採用支援01');
});

test('面接が1件も無い取引と、期間を指定した応募ゼロ（面接の数が空・応募日が不明な取引は別に数える）', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  const urls: string[] = [];
  // 100: 面接 0・期間 0 件、101: 面接が空・期間 2 件、102: 面接 0・応募日が不明、103: 面接 3・求人票なし
  const rows = [
    { ...boardRows[0], interviews: 0, listing_count: 2, contract_applications: 0, period_applications: 0, first_contract: true },
    { ...boardRows[1], interviews: null, listing_count: 1, contract_applications: 5, period_applications: 2, first_contract: true },
    { ...boardRows[2], interviews: 0, listing_count: 1, contract_applications: null, period_applications: null },
    { ...boardRows[3], interviews: 3, listing_count: null },
  ];
  await page.route('**/api/job-copy/deals?*', route => { urls.push(route.request().url()); return route.fulfill({ json: { ...boardFixture, rows, work: demoOwnerTodo } }); });
  await page.route('**/api/job-copy/consultant-good?*', route => route.fulfill({ json: { status: 'ready', data_day: '2026-10-15', previous_day: null, comparison: 'unavailable', rows: [] } }));
  await login(page); await page.goto('/app/job-copy?view=deals&owner=11&owner_field=consultant');
  const table = page.locator('.db-table');
  await expect(table.locator('tbody > tr')).toHaveCount(4);
  await page.getByRole('checkbox', { name: '面接が1件も無い取引だけ' }).check();
  await expect(page).toHaveURL(/deals_interview=zero/);
  await expect(table.locator('tbody > tr')).toHaveCount(2);
  await expect(table.locator('tbody > tr').nth(0)).toContainText('架空の採用支援01');
  await expect(table.locator('tbody > tr').nth(1)).toContainText('架空の採用支援03');
  await expect(page.getByText('面接の数が入っていない1件は、0件とは別に数えて含めていません。')).toBeVisible();
  await page.getByRole('checkbox', { name: '面接が1件も無い取引だけ' }).uncheck();
  await page.getByRole('checkbox', { name: '応募が1件も無い取引だけ' }).check();
  await page.getByRole('combobox', { name: '応募ゼロを調べる期間' }).selectOption('weeks');
  await expect(table.locator('tbody > tr')).toHaveCount(1);
  await expect(table.locator('tbody > tr').first()).toContainText('架空の採用支援01');
  await expect(page.getByText('応募日も作成日も分からない応募がある1件は、期間の応募数を数えられないため含みません。')).toBeVisible();
  expect(urls.some(url => /period_from=\d{4}-\d{2}-\d{2}&period_to=\d{4}-\d{2}-\d{2}/.test(url))).toBe(true);
  await page.getByRole('combobox', { name: '応募ゼロを調べる期間' }).selectOption('contract');
  await expect(page.getByRole('heading', { name: /初回契約の期間中/ })).toBeVisible();
  await expect(table.locator('tbody > tr')).toHaveCount(1);
  await page.getByRole('combobox', { name: '応募ゼロを調べる期間' }).selectOption('dates');
  await page.getByLabel('から').fill('2026-10-01');
  await page.getByLabel('まで').fill('2026-10-15');
  await expect.poll(() => urls.some(url => url.includes('period_from=2026-10-01') && url.includes('period_to=2026-10-15'))).toBe(true);
});
