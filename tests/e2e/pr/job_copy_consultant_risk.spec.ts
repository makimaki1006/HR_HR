import { expect, test } from '@playwright/test';
import { login } from './helpers/login';
import { demoHome } from '../../../frontend/src/screens/job-copy/demoHome';
import { demoOwnerTodo } from '../../../frontend/src/screens/job-copy/demoOwnerTodo';
import { detailFixture, riskFixture, riskRow, scopeFixture } from '../../../frontend/src/screens/job-copy/consultantRisk.testFixtures';

test('危ない案件・詳細・次の予定・担当切替・URLの往復', async ({ page }) => {
  await page.route('**/api/job-copy/consultant-scope?*', route => route.fulfill({ json: { ...scopeFixture, owner: new URL(route.request().url()).searchParams.get('owner') ?? '11' } }));
  await page.route('**/api/job-copy/consultant-risk?*', route => {
    const other = new URL(route.request().url()).searchParams.get('owner') === '22';
    return route.fulfill({ json: { ...riskFixture, rows: other ? [{ ...riskRow, deal_id: '102', name: '架空の採用支援B', consultant: '架空の担当B', company: '架空の運輸B' }] : riskFixture.rows } });
  });
  await page.route('**/api/consulting/deal-detail?*', route => route.fulfill({ json: detailFixture }));
  await page.route('**/api/job-copy/listings?*', route => route.fulfill({ json: { status: 'ready', total: 1, next_offset: null, listings: [{ id: '501' }] } }));
  await page.route('**/api/crm/workspace/deals/101?*', route => route.fulfill({ json: { hubspot_portal_id: '123', selected: { deal: { actionyotei: '2026-10-21', actionnaiyou: 'call' } }, partial: [] } }));
  await page.route('**/api/crm/deals/101/edit-schema', route => route.fulfill({ json: { writes_enabled: true, editable: [{ object: 'deal', name: 'actionyotei', type: 'date' }, { object: 'deal', name: 'actionnaiyou', type: 'enumeration', options: [{ value: 'call', label: '電話' }] }] } }));
  await login(page);
  await page.goto('/app/job-copy?view=risk&owner=11&owner_field=consultant');
  // 既定は名札2本以上（1本だけの2件は件数だけ出して外す）
  await expect(page.getByRole('note')).toContainText('名札が2本以上の案件 1件を出しています');
  await expect(page.getByRole('note')).toContainText('名札が1本だけの案件 2件は出していません');
  await expect(page.getByRole('button', { name: riskRow.name })).toHaveCount(1);
  await expect(page.getByRole('button', { name: '架空の採用支援C' })).toHaveCount(0);
  await page.getByRole('checkbox', { name: '名札が1本だけの案件も出す' }).check();
  await expect(page).toHaveURL(/risk_all=1/);
  const fresh = page.getByRole('region', { name: '今日わかった' });
  const ongoing = page.getByRole('region', { name: '前から出ていて直っていない' });
  await expect(fresh.getByRole('heading', { level: 2 })).toHaveText('今日わかった 2件');
  await expect(ongoing.getByRole('heading', { level: 2 })).toHaveText('前から出ていて直っていない 1件');
  const mixed = fresh.getByRole('row').filter({ has: page.getByRole('button', { name: riskRow.name }) });
  await expect(mixed.getByRole('listitem')).toHaveText(['NPSが4以下今日わかった', '満了まで60日以内']);
  await expect(mixed.getByRole('listitem').nth(0)).toHaveClass('cr-new');
  await expect(mixed.getByRole('listitem').nth(1)).not.toHaveClass('cr-new');
  await expect(mixed.getByRole('cell').nth(3)).toHaveText('1日前');
  await expect(mixed.getByText('2026/10/16から')).toHaveCount(1);
  await expect(page.getByRole('button', { name: riskRow.name })).toHaveCount(1);
  await expect(ongoing.getByRole('button')).toHaveText(['架空の採用支援C']);
  await expect(page.getByRole('combobox', { name: '担当者', exact: true })).toHaveCount(1);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  const dir = process.env.CONSULTANT_SCREENSHOT_DIR;
  if (dir) await page.screenshot({ path: `${dir}/risk-list.png`, fullPage: false });
  await fresh.getByRole('button', { name: riskRow.name }).click();
  const detail = page.getByRole('dialog');
  await expect(detail.getByText('採用状況を確認し、次の打ち合わせを決めた。')).toBeVisible();
  await expect(detail.getByText('接触：1日前 ／ MTG：2026/10/01')).toBeVisible();
  await expect(detail.getByRole('heading', { name: '2026/10/18 電話' })).toBeVisible();
  await expect(detail.getByRole('link', { name: '求人を見る' })).toHaveAttribute('href', /deal=101&listing=501/);
  await expect(detail.getByRole('link', { name: 'HubSpotで開く' })).toHaveAttribute('href', /record\/0-3\/101$/);
  expect(await detail.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
  if (dir) await page.screenshot({ path: `${dir}/risk-detail.png`, fullPage: false });
  await detail.getByRole('button', { name: '終わった → 次の予定' }).click();
  const plan = page.getByRole('dialog', { name: '次の予定を入れる' });
  await expect(plan.getByLabel('日付')).toBeVisible(); await expect(plan.getByLabel('内容')).toBeVisible();
  await plan.getByRole('button', { name: '閉じる' }).click();
  await detail.getByRole('button', { name: '閉じる' }).click();
  await page.getByRole('combobox', { name: '担当者', exact: true }).selectOption('22');
  await expect(fresh).toContainText('架空の採用支援B'); await expect(fresh).not.toContainText('架空の採用支援A');
  await page.reload(); await expect(page.getByRole('combobox', { name: '担当者', exact: true })).toHaveValue('22');
  await expect(fresh).toContainText('架空の採用支援B');
  if (dir) await page.screenshot({ path: `${dir}/risk-owner.png`, fullPage: false });
  await page.getByRole('button', { name: '案件一覧', exact: true }).click();
  await expect(page).toHaveURL(/owner=22/); await expect(page).toHaveURL(/view=deals/);
  await page.goBack(); await expect(fresh).toContainText('架空の採用支援B');
  await expect(page.getByRole('combobox', { name: '担当の項目' })).toHaveValue('consultant');
});

// 共通メニューのホームは、固定一覧・求人の選択状態も解除する。
test('固定一覧・求人詳細からホームへ戻り、担当を保つ', async ({ page }) => {
  await page.route('**/api/job-copy/home*', route => route.fulfill({ json: { ...demoHome, work: { ...demoOwnerTodo, owner: '22' } } }));
  await page.route('**/api/job-copy/recent-changes*', route => route.fulfill({ json: { status: 'ready', report: { count: 0, listings: [] } } }));
  await page.route('**/api/job-copy/moc', route => route.fulfill({ json: { schemaVersion: 1, capturedAt: '2026-10-19T00:00:00Z', capture_bundle: { schemaVersion: 1, jobs: [] }, results: [] } }));
  await page.route('**/api/job-copy/listings/501/versions', route => route.fulfill({ status: 503, json: {} }));
  await login(page);
  for (const query of ['source=fixed&data=actual', 'listing=501']) {
    await page.goto(`/app/job-copy?${query}&owner=22&owner_field=consultant`);
    await page.getByRole('navigation', { name: '案件管理のメニュー' }).getByRole('button', { name: 'ホーム', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'ホーム', exact: true })).toBeVisible();
    await expect(page.getByRole('combobox', { name: '担当者', exact: true })).toHaveValue('22');
    expect(new URL(page.url()).searchParams.has('listing')).toBe(false);
    expect(new URL(page.url()).searchParams.has('source')).toBe(false);
    expect(new URL(page.url()).searchParams.has('data')).toBe(false);
  }
});


test('求人作業中の担当選択を開いて切り替え、Escで閉じる', async ({ page }) => {
  await page.route('**/api/job-copy/consultant-scope?*', route => route.fulfill({ json: { ...scopeFixture, owner: new URL(route.request().url()).searchParams.get('owner') ?? '11' } }));
  await page.route('**/api/job-copy/moc', route => route.fulfill({ json: { schemaVersion: 1, capturedAt: '2026-10-19T00:00:00Z', capture_bundle: { schemaVersion: 1, jobs: [] }, results: [] } }));
  await login(page);
  await page.goto('/app/job-copy?source=fixed&data=actual&owner=11&owner_field=consultant');
  const owner = page.getByRole('combobox', { name: '担当者', exact: true });
  await expect(owner).toHaveCount(0);
  await page.getByRole('button', { name: '担当者で絞る', exact: true }).click();
  await expect(owner).toHaveValue('11');
  await owner.selectOption('22');
  await expect(owner).toHaveValue('22');
  await expect(page).toHaveURL(/owner=22/);
  await expect(page).toHaveURL(/source=fixed/);
  await page.keyboard.press('Escape');
  await expect(page.getByRole('region', { name: '担当者の選択' })).toBeHidden();
});
