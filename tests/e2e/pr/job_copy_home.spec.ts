import { expect, test } from '@playwright/test';
import { demoOwnerTodo, demoOwnerTodoVolume } from '../../../frontend/src/screens/job-copy/demoOwnerTodo';
import { demoHome } from '../../../frontend/src/screens/job-copy/demoHome';
import { login } from './helpers/login';
import { E2E_EMAIL } from './helpers/fixture_values';

test.use({ locale: 'ja-JP' });

// 外部サービスを使わず、5分類すべてと補助カードの合成データを返す。
test('ホームの8枚のカードと、変更求人の一覧・求人と案件への移動を確認する', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1500 });
  await page.route('**/api/job-copy/recent-changes', route => route.fulfill({ json: { status: 'ready', refresh_failed: false, report: { count: 3, listings: [
    { id: '1', title: '合成終了求人', media: 'airwork', publication_status: '掲載終了', deals: [{ id: '10', name: '合成案件A', company: '合成事業所' }, { id: '20', name: '合成案件B', company: '合成事業所' }], fields: ['本文', '給与', '求人名'], last_changed_at: '2026-10-15T16:12:00Z' },
    { id: '3', title: '合成配送求人', media: 'hrh', publication_status: '掲載中', deals: [{ id: '20', name: '合成案件B', company: '合成事業所' }], fields: ['本文'], last_changed_at: '2026-10-12T03:00:00Z' },
    { id: '2', title: '合成掲載求人', media: 'hrh', publication_status: '掲載中', deals: [{ id: '10', name: '合成案件A', company: null }], fields: ['給与'], last_changed_at: '2026-10-09T15:00:00Z' },
  ] } } }));
  await page.route('**/api/job-copy/home', route => route.fulfill({ json: demoHome }));
  await page.route('**/api/job-copy/moc', route => route.fulfill({ json: { schemaVersion: 1, capturedAt: '2026-10-15T01:00:00Z', capture_bundle: { schemaVersion: 1, capturedAt: '2026-10-15T01:00:00Z', jobs: [] }, results: [] } }));
  await login(page);
  await page.goto('/app/job-copy');
  const cards = page.locator('.jh-card');
  await expect(cards).toHaveCount(8);
  await expect(page.getByRole('button', { name: /市場の職種が未指定の求人/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /直近7日に変更/ }).locator('.jh-count')).toHaveText('3件');
  for (const [index, card] of demoHome.home.cards.entries()) {
    await expect(cards.nth(index).locator('.jh-count')).toHaveText(`${card.count}件`);
    await expect(cards.nth(index).locator('.jh-card-title')).toHaveText(card.title);
    await expect(cards.nth(index).locator('.jh-rule')).toHaveText(card.rule);
  }
  await expect(cards.nth(0)).toHaveClass(/jh-alert/);
  await expect(cards.nth(1)).toHaveClass(/jh-alert/);
  await expect(page.getByText(/2件以上、または20%以上の差を増減/)).toBeVisible();
  for (const [index, applications] of ['0 → 0 件', '1 → 0 件', '5 → 4 件', '0 → 1 件', '10 → 9 件'].entries()) {
    await cards.nth(index).click();
    const list = page.getByRole('region', { name: `${demoHome.home.cards[index]!.title}の一覧` });
    await expect(list.getByRole('cell', { name: applications, exact: true }).first()).toBeVisible();
  }
  await cards.nth(0).click();
  const list = page.getByRole('region', { name: '応募がゼロのままの求人の一覧' });
  await expect(list.getByRole('columnheader', { name: '課金（直近 4 週）' })).toBeVisible();
  await expect(list.locator('tbody tr').nth(0)).toContainText('¥24,000');
  await expect(list.locator('tbody tr').nth(1)).toContainText('¥0');
  await expect(page.getByRole('columnheader', { name: '応募（前の 4 週 → 直近 4 週）' })).toBeVisible();
  await page.getByRole('button', { name: /直近7日に変更/ }).click();
  const changes = page.getByRole('region', { name: '直近7日に変更の一覧' });
  await expect(changes.locator('tbody tr')).toHaveCount(3);
  await expect(changes.locator('tbody tr').nth(0)).toHaveText('合成事業所合成案件A・合成案件B合成終了求人AirWork掲載終了本文・給与・求人名2026/10/16 01:12');
  await expect(changes.locator('tbody tr').nth(1)).toHaveText('合成事業所合成案件B合成配送求人HRハッカー掲載中本文2026/10/12 12:00');
  await expect(changes.locator('tbody tr').nth(2)).toHaveText('—合成案件A合成掲載求人HRハッカー掲載中給与2026/10/10 00:00');
  if (process.env.HOME_TREND_SCREENSHOT) {
    // 認証用の合成メールアドレスも写さず、ホームの主領域を保存する。
    await page.mouse.move(1400, 100);
    await page.keyboard.press('Escape');
    await expect(page.locator('.jc-sidebar > .jc-list')).toBeHidden();
    await page.locator('.jc-workspace').screenshot({ path: process.env.HOME_TREND_SCREENSHOT });
  }
  await page.route('**/api/job-copy/listings?*', route => route.fulfill({ json: { status: 'ready', total: 0, listings: [], deals: [], titles: [], index_built_at: '2026-10-16T00:00:00Z' } }));
  const ended = { id: '1', title: '合成終了求人', media: 'airwork', media_job_id: 'synthetic', account_id: null, publication_status: '掲載終了', prefecture: null, municipality: null, category: null, last_csv_detected_at: null, application_count: 0 };
  await page.route('**/api/job-copy/listings/1/versions', route => route.fulfill({ json: { listing: ended, current_fields: {}, versions: [{ written_at: '2026-10-15T16:12:00Z', body: '仕事内容：合成の検証用求人です。', image_urls: [] }], history_counts: {}, history_may_be_incomplete: false } }));
  await changes.getByRole('button', { name: '合成終了求人' }).click();
  await expect(page).toHaveURL(/listing=1/);
  await expect(page.getByRole('heading', { name: '合成終了求人', exact: true })).toBeVisible();
  await expect(page.locator('.jc-detail-heading')).toContainText('掲載終了');
  await page.goBack();
  await expect(changes).toBeVisible();
  expect(new URL(page.url()).searchParams.get('card')).toBe('recent_changes');
  await expect(changes.locator('tbody tr')).toHaveCount(3);
  await page.goForward();
  await expect(page.getByRole('heading', { name: '合成終了求人', exact: true })).toBeVisible();
  await page.goBack();
  await changes.getByRole('button', { name: '合成案件A' }).first().click();
  await expect(page).toHaveURL(/deal=10/);
  await expect(page.getByRole('heading', { name: 'この取引の求人', exact: true })).toBeVisible();
  await expect(page.locator('.jc-deal-comparison > header p')).toHaveText('合成案件A');
  await page.goBack();
  await expect(changes).toBeVisible();
  expect(new URL(page.url()).searchParams.get('card')).toBe('recent_changes');
  await expect(changes.locator('tbody tr')).toHaveCount(3);
  await page.goForward();
  await expect(page.getByRole('heading', { name: 'この取引の求人', exact: true })).toBeVisible();
});

test('担当者の切替でホームのカードを絞り、案件・会社から戻って一覧を保つ', async ({ page }) => {
  await page.route(/\/api\/job-copy\/home(?:\?|$)/, route => {
    const query = new URL(route.request().url()).searchParams;
    const owner = query.get('owner') === 'all' ? 'all' : query.get('owner') === '22' ? '22' : '11';
    const field = query.get('owner_field') ?? 'consultant';
    const empty = owner === '22';
    return route.fulfill({ json: { ...demoHome, work: { ...demoOwnerTodo, owner, owner_field: field,
      actions: empty ? { overdue: [], today: [], week: [], later: [] } : demoOwnerTodo.actions, expirations: empty ? [] : demoOwnerTodo.expirations },
      home: { ...demoHome.home, cards: demoHome.home.cards.map(card => empty ? { ...card, count: 0, listings: [] } : card), customers: empty ? [] : demoHome.home.customers } } });
  });
  await page.route('**/api/job-copy/recent-changes*', route => route.fulfill({ json: { status: 'ready', refresh_failed: false, report: { count: 0, listings: [] } } }));
  await page.route('**/api/job-copy/moc', route => route.fulfill({ json: { schemaVersion: 1, capturedAt: '2026-10-16T00:00:00Z', capture_bundle: { schemaVersion: 1, capturedAt: '2026-10-16T00:00:00Z', jobs: [] }, results: [] } }));
  await page.route('**/api/job-copy/listings?*', route => route.fulfill({ json: { status: 'ready', total: 0, listings: [], deals: [], titles: [], index_built_at: '2026-10-16T00:00:00Z' } }));
  await page.route('**/api/job-copy/company-deals?*', route => route.fulfill({ json: { company: { id: '901', name: '合成事業所' }, deals: [], index_ready: true } }));
  await login(page);
  await page.goto('/app/job-copy');
  await expect(page.getByRole('combobox', { name: '担当者', exact: true })).toHaveValue('11');
  await expect(page.getByLabel('担当の項目')).toHaveValue('consultant');
  const overdueCard = page.locator('.jh-todo-cards').getByRole('button', { name: /期限切れ/ });
  await expect(overdueCard.locator('.jh-count')).toHaveText('2件');
  await expect(page.locator('.jh-todo-table')).toHaveCount(0);
  await overdueCard.click();
  const overdue = page.getByRole('region', { name: '期限切れの一覧' });
  await expect(overdue.locator('tbody tr')).toHaveCount(2);
  await expect(overdue.locator('tbody tr').first()).toContainText('合成事業所合成案件101対応中2026/10/15電話する合成担当A');
  await page.getByRole('combobox', { name: '担当者', exact: true }).selectOption('22');
  await expect(overdueCard.locator('.jh-count')).toHaveText('0件');
  await expect(page.locator('.jh-customer-table tbody')).toContainText('契約中の案件がある顧客はありません');
  await page.getByRole('button', { name: '自分に戻す' }).click();
  await expect(page.getByRole('combobox', { name: '担当者', exact: true })).toHaveValue('11');
  await expect(overdueCard.locator('.jh-count')).toHaveText('2件');
  await page.getByLabel('担当の項目').selectOption('hubspot_owner_id');
  await expect(page).toHaveURL(/owner_field=hubspot_owner_id/);
  await page.getByRole('combobox', { name: '担当者', exact: true }).selectOption('all');
  await expect(page.getByRole('combobox', { name: '担当者', exact: true })).toHaveValue('all');
  await overdue.getByRole('button', { name: '合成案件101' }).click();
  await expect(page).toHaveURL(/deal=101/);
  expect(new URL(page.url()).searchParams.get('owner')).toBe('all');
  await page.goBack();
  await expect(overdue).toBeVisible();
  await overdue.getByRole('button', { name: '合成事業所' }).first().click();
  await expect(page).toHaveURL(/company=901/);
  await page.goBack();
  await expect(overdue).toBeVisible();
  await expect(page.getByRole('combobox', { name: '担当者', exact: true })).toHaveValue('all');
});

test('本人が解決できる既定画面と、198件の期限切れを20件ずつ開いた画面を日本語で確認する', async ({ page }) => {
  const syntheticOwners = [{ id: '11', email: E2E_EMAIL }, { id: '22', email: 'former@example.test' }];
  const self = syntheticOwners.find(owner => owner.email === E2E_EMAIL)?.id;
  expect(self).toBe('11');
  await page.setViewportSize({ width: 1440, height: 1600 });
  await page.route(/\/api\/job-copy\/home(?:\?|$)/, route => {
    const query = new URL(route.request().url()).searchParams;
    const selected = query.get('owner');
    return route.fulfill({ json: { ...demoHome, work: { ...demoOwnerTodoVolume, owner: !selected || selected === 'me' ? self : selected, owner_field: query.get('owner_field') ?? 'consultant' } } });
  });
  await page.route('**/api/job-copy/recent-changes*', route => route.fulfill({ json: { status: 'ready', refresh_failed: false, report: { count: 0, listings: [] } } }));
  await page.route('**/api/job-copy/moc', route => route.fulfill({ json: { schemaVersion: 1, capturedAt: '2026-10-16T00:00:00Z', capture_bundle: { schemaVersion: 1, capturedAt: '2026-10-16T00:00:00Z', jobs: [] }, results: [] } }));
  await login(page);
  await page.goto('/app/job-copy');
  expect(await page.evaluate(() => navigator.language)).toBe('ja-JP');
  await expect(page.getByRole('combobox', { name: '担当者', exact: true })).toHaveValue('11');
  await expect(page.getByRole('combobox', { name: '担当の項目' })).toHaveValue('consultant');
  await expect(page).toHaveURL(/owner=11/);
  const cards = page.locator('.jh-todo-cards .jh-card');
  await expect(cards).toHaveCount(7);
  for (const [index, count] of [1, 1, 1, 1, 198, 95, 112].entries()) await expect(cards.nth(index).locator('.jh-count')).toHaveText(`${count}件`);
  await expect(page.locator('.jh-todo-table')).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'まず見るもの' })).toBeInViewport();
  const boxes = await cards.evaluateAll(elements => elements.map(el => ({ top: el.getBoundingClientRect().top, height: el.getBoundingClientRect().height })));
  expect(new Set(boxes.map(box => Math.round(box.top))).size).toBe(2);
  const filterLayout = await page.locator('.jh-owner').evaluate(el => {
    const controls = [...el.querySelectorAll('label')];
    return controls.map(label => ({ direction: getComputedStyle(label).flexDirection, top: label.getBoundingClientRect().top, height: label.getBoundingClientRect().height }));
  });
  expect(filterLayout.every(label => label.direction === 'row')).toBe(true);
  expect(new Set(filterLayout.map(label => Math.round(label.top))).size).toBe(1);
  const dir = process.env.OWNER_TODO_SCREENSHOT_DIR;
  if (dir) await page.locator('.jc-main').screenshot({ path: `${dir}/default.png` });
  await cards.nth(4).click();
  const list = page.getByRole('region', { name: '期限切れの一覧' });
  await expect(list.locator('tbody tr')).toHaveCount(20);
  await expect(list).toContainText('1〜20件 ／ 全198件');
  await expect(list.locator('tbody tr').first()).toContainText('合成案件1000対応中2026/10/15応募状況を確認する合成担当A');
  if (dir) await page.locator('.jc-main').screenshot({ path: `${dir}/overdue.png` });
  await list.getByRole('button', { name: '次の20件' }).click();
  await expect(list).toContainText('21〜40件 ／ 全198件');
  await expect(list.locator('tbody tr').first()).toContainText('合成案件1020');
  await expect(page).toHaveURL(/todo_page=2/);
  await page.goBack();
  await expect(list).toContainText('1〜20件 ／ 全198件');
  await page.goBack();
  await expect(page.locator('.jh-todo-table')).toHaveCount(0);
  await page.goForward();
  await expect(list.locator('tbody tr')).toHaveCount(20);
  await page.locator('.jh-todo-cards').getByRole('button', { name: /契約満了が近い/ }).click();
  const expiration = page.getByRole('region', { name: '契約満了が近いの一覧' });
  await expect(expiration.locator('tbody tr')).toHaveCount(1);
  await expect(expiration).toContainText('2026/11/15');
  await page.getByRole('button', { name: 'それ以降の予定（154件）' }).click();
  await expect(page.getByRole('region', { name: 'それ以降の一覧' }).locator('tbody tr')).toHaveCount(20);
});

test('職種の一括指定は URL から引き続き開ける', async ({ page }) => {
  await page.route('**/api/job-copy/listings?*', route => route.fulfill({ json: {
    status: 'ready', listings: [], total: 0, index_built_at: '2026-10-16T00:00:00Z',
    offset: 0, next_offset: null, titles: [], refreshing: false, refresh_failed: false,
    indeed_title_counts: { set: 0, none: 0, unset: 0 },
  } }));
  await login(page);
  await page.goto('/app/job-copy?view=occupations');
  await expect(page.getByText('市場と比べる職種を指定する', { exact: true })).toBeVisible();
  await expect(page.getByRole('tab', { name: '未指定（0）' })).toBeVisible();
  await expect(page.getByText('職種が未指定の求人はありません。', { exact: true })).toBeVisible();
});
