import { expect, test } from '@playwright/test';
import { demoHome } from '../../../frontend/src/screens/job-copy/demoHome';
import { demoOwnerTodo } from '../../../frontend/src/screens/job-copy/demoOwnerTodo';
import { contractCatalog, contractDetail, contractSchema, contractValues } from '../../../frontend/src/screens/job-copy/dealContract.testFixtures';
import { login } from './helpers/login';

test.use({ locale: 'ja-JP' });
test('要対応の理由をまとめ、予定をvalueで保存し、ホームと案件に反映する', async ({ page }) => {
  await page.setViewportSize({ width: 1580, height: 1300 });
  let due = '2026-10-15'; let content = 'old';
  const patches: { base: unknown; set: unknown; operation_id: string }[] = [];
  const options = [{ label: '応募状況を確認する', value: 'review_value' }, { label: '電話する', value: 'old' }];
  const reasons = demoOwnerTodo.signals![0]!.reasons!;
  await page.route('**/api/crm/edit-schema?*', route => route.fulfill({ json: { ...contractSchema, editable: [...contractSchema.editable,
    { object: 'deal', name: 'actionyotei', label: '予定日', type: 'date', field_type: 'date', options: [] },
    { object: 'deal', name: 'actionnaiyou', label: '内容', type: 'enumeration', field_type: 'select', options },
  ] } }));
  await page.route('**/api/crm/property-catalog', route => route.fulfill({ json: contractCatalog }));
  await page.route('**/api/crm/workspace/deals/*', route => route.fulfill({ json: contractDetail({ ...contractValues, contract_start_date: '2026-08-17', contract_expiration_date: '2026-11-15', ...({ actionyotei: due, actionnaiyou: content }) }) }));
  await page.route('**/api/crm/deals/106', async route => {
    expect(route.request().method()).toBe('PATCH');
    const body = route.request().postDataJSON(); patches.push(body); due = body.set.actionyotei; content = body.set.actionnaiyou;
    await route.fulfill({ json: { status: 'saved', operation_id: body.operation_id, values: body.set, objects_values: {} } });
  });
  await page.route('**/api/job-copy/deals/106/refresh', route => route.fulfill({ json: { status: 'refreshed' } }));
  await page.route('**/api/job-copy/deals/106/todo', route => route.fulfill({ json: { status: 'ready', reasons, refresh_failed: false } }));
  await page.route('**/api/job-copy/home*', route => {
    const signal = { ...demoOwnerTodo.signals![0]!, actionyotei: due, content: options.find(o => o.value === content)?.label ?? '' };
    return route.fulfill({ json: { ...demoHome, work: { ...demoOwnerTodo, signals: [signal], actions: { overdue: due === '2026-10-15' ? [signal] : [], today: [], week: [], later: due !== '2026-10-15' ? [signal] : [] } } } });
  });
  await page.route('**/api/job-copy/recent-changes*', route => route.fulfill({ json: { status: 'ready', refresh_failed: false, report: { count: 0, listings: [] } } }));
  await page.route('**/api/job-copy/moc', route => route.fulfill({ json: { schemaVersion: 1, capturedAt: '2026-10-16T00:00:00Z', capture_bundle: { schemaVersion: 1, capturedAt: '2026-10-16T00:00:00Z', jobs: [] }, results: [] } }));
  await page.route('**/api/job-copy/listings?*', route => route.fulfill({ json: { status: 'ready', total: 0, listings: [], deals: [], titles: [], index_built_at: '2026-10-16T00:00:00Z' } }));
  await page.route('**/api/job-copy/company-deals?*', route => route.fulfill({ json: { company: { id: '901', name: '合成事業所' }, deals: [], index_ready: true } }));
  const dir = process.env.TODO_SIGNALS_SCREENSHOT_DIR;
  await login(page); await page.goto('/app/job-copy');
  await expect(page.locator('.jh-todo-cards .jh-card')).toHaveCount(7);
  for (const card of ['契約満了が近い', '接触が空いている', '応募が止まっている', '初回で成果が出ていない']) await expect(page.getByRole('button', { name: new RegExp(card) }).locator('.jh-count')).toHaveText('1件');
  if (dir) await page.locator('.jc-main').screenshot({ path: `${dir}/home.png` });
  await page.getByRole('button', { name: /接触が空いている/ }).click();
  const list = page.getByRole('region', { name: '接触が空いているの一覧' });
  await expect(list.locator('tbody tr')).toHaveCount(1);
  await expect(list.locator('.nt-reason')).toHaveText(reasons.map(r => r.text));
  if (dir) await page.locator('.jc-main').screenshot({ path: `${dir}/signals.png` });
  await list.getByRole('button', { name: '終わった → 次の予定' }).click();
  const dialog = page.getByRole('dialog', { name: '次の予定を入れる' });
  await dialog.getByLabel('日付').fill('2026/10/23');
  await dialog.getByRole('combobox', { name: '内容', exact: true }).selectOption('review_value');
  if (dir) await dialog.screenshot({ path: `${dir}/next-plan.png` });
  await dialog.getByRole('button', { name: '保存', exact: true }).click();
  await expect(dialog.getByText('保存済み', { exact: true })).toBeVisible();
  expect(patches).toHaveLength(1);
  expect(patches[0]!.base).toEqual({ actionyotei: '2026-10-15', actionnaiyou: 'old' });
  expect(patches[0]!.set).toEqual({ actionyotei: '2026-10-23', actionnaiyou: 'review_value' });
  expect(patches[0]!.operation_id).toMatch(/^[0-9a-f-]{36}$/);
  await dialog.getByRole('button', { name: '閉じる' }).click();
  await expect(list.locator('tbody tr')).toContainText('2026/10/23応募状況を確認する');
  await expect(page.getByRole('button', { name: /期限切れ/ }).locator('.jh-count')).toHaveText('0件');
  await list.getByRole('button', { name: '合成案件106' }).click();
  const next = page.getByRole('region', { name: '次にやること' });
  await expect(next).toContainText('2026/10/23 ・ 応募状況を確認する');
  await expect(next.locator('.nt-reason')).toHaveText(reasons.map(r => r.text));
  await expect(page.getByRole('region', { name: '契約・成果', exact: true }).getByText('¥240,000', { exact: true })).toBeVisible();
  if (dir) await page.locator('.jc-main').screenshot({ path: `${dir}/deal.png` });
});
