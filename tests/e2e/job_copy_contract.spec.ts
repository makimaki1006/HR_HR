import { expect, test } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { contractCatalog, contractDetail, contractSchema, contractValues, stageSchema } from '../../frontend/src/screens/job-copy/dealContract.testFixtures';

import { demoHome } from '../../frontend/src/screens/job-copy/demoHome';

const shots = 'docs/screenshots/job-copy-contract';
test('案件画面で契約を保存して緑、競合では現在値を確認して選ぶ', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1100 });
  await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
  let values = { ...contractValues, stageSchema };
  const patches: { operation_id: string; base: Record<string, string>; set: Record<string, string> }[] = [];
  let conflict = false;
  let detailReads = 0;
  let homeReads = 0;
  await page.route('**/api/crm/**', async route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/property-catalog')) return route.fulfill({ json: contractCatalog });
    if (url.pathname.endsWith('/edit-schema')) return route.fulfill({ json: contractSchema });
    if (url.pathname.includes('/workspace/deals/')) { detailReads++; return route.fulfill({ json: contractDetail(values) }); }
    if (route.request().method() === 'PATCH') {
      const body = route.request().postDataJSON(); patches.push(body);
      expect(route.request().headers()['x-requested-with']).toBe('fetch');
      if (conflict) return route.fulfill({ status: 409, json: { status: 'conflict', object: 'deal', current: { amount: '420000' }, changed_by_hubspot: ['amount'] } });
      values = { ...values, ...body.set };
      return route.fulfill({ json: { status: 'saved', operation_id: body.operation_id, values: body.set, objects_values: {} } });
    }
    return route.fulfill({ status: 503, json: { error: 'synthetic_unavailable' } });
  });
  await page.route('**/api/job-copy/**', route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/home')) { homeReads++; return route.fulfill({ json: demoHome }); }
    if (url.pathname.endsWith('/recent-changes')) return route.fulfill({ json: { status: 'ready', report: { count: 1, listings: [{ id: '301', title: '合成求人', media: 'hrh', publication_status: '掲載中', fields: ['求人名'], last_changed_at: '2026-10-16T00:00:00Z', deals: [{ id: '901', name: '合成・契約確認案件', company: '合成事業所' }] }] }, refresh_failed: false } });
    if (url.pathname.endsWith('/listings')) return route.fulfill({ json: { status: 'ready', listings: [], total: 0, titles: [], deals: [{ id: '901', name: '合成・契約確認案件', count: 0 }], offset: 0, next_offset: null, index_built_at: '2026-10-16T00:00:00Z' } });
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  await page.goto('/app/job-copy?source=hubspot');
  await page.locator('.jh-card').filter({ hasText: '直近7日に変更' }).click();
  await page.getByRole('button', { name: '合成・契約確認案件', exact: true }).click();
  const initialHomeReads = homeReads;
  const section = page.getByRole('region', { name: '契約・成果', exact: true });
  await expect(section).toContainText('¥240,000');
  await expect(section).toContainText('¥230,000');
  await expect(section).toContainText('¥19,167');
  await expect(section).toContainText('2026/10/01');
  expect(await page.evaluate(() => navigator.language)).toBe('ja-JP');
  expect(await page.evaluate(() => Intl.DateTimeFormat().resolvedOptions().timeZone)).toBe('Asia/Tokyo');
  await section.getByRole('button', { name: '編集', exact: true }).click();
  await section.getByLabel('金額', { exact: true }).fill('360000');
  await section.getByLabel('契約プラン', { exact: true }).selectOption('premium');
  await section.getByLabel('契約開始日', { exact: true }).fill('2026-11-01');
  await section.getByLabel('応募数', { exact: true }).fill('25');
  await section.getByRole('button', { name: '保存', exact: true }).click();
  await expect(section.locator('.dc-green')).toHaveCount(1);
  await expect(section).toContainText('¥360,000');
  await expect(section).toContainText('合成・充実');
  expect(patches[0].base).toEqual({ amount: '240000', contract_plan: 'basic', contract_start_date: '2026-10-01T00:00:00Z', oubo: '20' });
  expect(patches[0].set).toEqual({ amount: '360000', contract_plan: 'premium', contract_start_date: '2026-11-01', oubo: '25' });
  expect(patches[0].operation_id).toMatch(/^[a-zA-Z0-9_-]{8,64}$/);
  await expect.poll(() => detailReads).toBe(2);
  await mkdir(shots, { recursive: true });
  await expect(section.locator('.dc-written').first()).toHaveCSS('animation-duration', '3s');
  await section.screenshot({ path: `${shots}/01-saved.png`, animations: 'disabled' });
  await expect(section.locator('.dc-field .dc-status')).toHaveCount(0);
  await expect(section.locator('.dc-written').first()).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
  conflict = true;
  await section.getByRole('button', { name: '編集', exact: true }).click();
  await section.getByLabel('金額', { exact: true }).fill('480000');
  await expect(section.getByText('元: ¥360,000', { exact: true })).toBeVisible();
  await expect(section.locator('.dc-original')).toHaveCount(1);
  await expect(section.locator('.dc-field strong')).toHaveCount(2);
  await section.screenshot({ path: `${shots}/02-editing.png`, animations: 'disabled' });
  await section.getByRole('button', { name: '保存', exact: true }).click();
  await expect(section.getByRole('alert')).toContainText('現在の値「¥420,000」／入力した値「¥480,000」');
  expect(patches).toHaveLength(2);
  expect(patches[1].base).toEqual({ amount: '360000' });
  await section.screenshot({ path: `${shots}/03-conflict.png`, animations: 'disabled' });
  await section.getByRole('button', { name: '現在の値を使う', exact: true }).click();
  await expect(section).toContainText('¥420,000');
  expect(patches).toHaveLength(2);
  await page.getByRole('navigation', { name: '現在地' }).getByRole('button', { name: '案件管理' }).click();
  await expect(page.getByRole('heading', { name: 'ホーム', exact: true })).toBeVisible();
  await expect.poll(() => homeReads).toBeGreaterThan(initialHomeReads);
});

test('ステージ移動で必須入力と未保存の契約を保存し、緑と再読込を確認する', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1100 });
  await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
  let values = { ...contractValues, oubo: '', mensetu: '' };
  let stageId = 'stage-demo';
  let homeReads = 0;
  let companyReads = 0;
  const patches: Record<string, unknown>[] = [];
  await page.route('**/api/crm/**', route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/property-catalog')) return route.fulfill({ json: contractCatalog });
    if (url.pathname.endsWith('/edit-schema')) return route.fulfill({ json: stageSchema });
    if (url.pathname.includes('/workspace/deals/')) return route.fulfill({ json: { ...contractDetail(values), deal: { ...contractDetail(values).deal, stage_id: stageId, stage_label: stageSchema.stages.find(s => s.id === stageId)?.label } } });
    if (route.request().method() === 'PATCH') {
      const body = route.request().postDataJSON(); patches.push(body);
      values = { ...values, ...body.set }; stageId = body.stage.stage_id;
      return route.fulfill({ json: { status: 'saved', operation_id: body.operation_id, values: body.set, objects_values: {} } });
    }
    return route.fulfill({ status: 503, json: { error: 'synthetic_unavailable' } });
  });
  await page.route('**/api/job-copy/**', route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/refresh')) return route.fulfill({ json: { status: 'refreshed' } });
    if (url.pathname.endsWith('/home')) { homeReads++; return route.fulfill({ json: { ...demoHome, home: { ...demoHome.home, customers: demoHome.home.customers.map(c => ({ ...c, active_deals: stageId === 'stage-demo' ? 1 : 0, latest_deal: { ...c.latest_deal, active: stageId === 'stage-demo', stage_label: stageSchema.stages.find(s => s.id === stageId)?.label } })) } } }); }
    if (url.pathname.endsWith('/company-deals')) { companyReads++; return route.fulfill({ json: { company: { id: '501', name: '合成事業所' }, deals: [{ id: '901', name: '合成・ステージ確認案件', pipeline_id: '21596025', stage_label: stageSchema.stages.find(s => s.id === stageId)?.label, active: stageId === 'stage-demo', current: false }] } }); }
    if (url.pathname.endsWith('/listing-body-dates')) return route.fulfill({ json: { dates: {} } });
    if (url.pathname.endsWith('/deal-applications')) return route.fulfill({ json: { listings: [], shared_applications: 0, company: { id: '501', name: '合成事業所' } } });
    if (url.pathname.endsWith('/recent-changes')) return route.fulfill({ json: { status: 'ready', report: { count: 1, listings: [{ id: '301', title: '合成求人', media: 'hrh', publication_status: '掲載中', fields: ['求人名'], last_changed_at: '2026-10-16T00:00:00Z', deals: [{ id: '901', name: '合成・ステージ確認案件', company: '合成事業所' }] }] }, refresh_failed: false } });
    if (url.pathname.endsWith('/listings')) return route.fulfill({ json: { status: 'ready', listings: [{ id: '301', title: '合成求人', media: 'hrh', publication_status: '掲載中' }], total: 1, titles: [], deals: [{ id: '901', name: '合成・ステージ確認案件', count: 0 }], offset: 0, next_offset: null } });
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  await page.goto('/app/job-copy?source=hubspot');
  await page.locator('.jh-card').filter({ hasText: '直近7日に変更' }).click();
  await page.getByRole('button', { name: '合成・ステージ確認案件', exact: true }).click();
  const section = page.getByRole('region', { name: '契約・成果', exact: true });
  const stage = section.getByLabel('ステージを変更');
  await expect(stage).toHaveValue('stage-demo');
  expect(await stage.locator('option').allTextContents()).toEqual(['合成・契約中', '継続済', '納品完了', '解約済（合成）']);
  expect(await page.evaluate(() => navigator.language)).toBe('ja-JP');
  const initialHomeReads = homeReads;
  await expect.poll(() => companyReads).toBe(1);
  await mkdir('docs/screenshots/job-copy-stage-move', { recursive: true });
  await stage.focus();
  // ネイティブのポップアップは撮影対象外なので、選択肢を同じselect内に展開して撮影する。
  await stage.evaluate(el => { (el as HTMLSelectElement).size = 4; });
  await section.screenshot({ path: 'docs/screenshots/job-copy-stage-move/01-stage-selection.png' });
  await stage.evaluate(el => { (el as HTMLSelectElement).size = 0; });
  await section.getByRole('button', { name: '編集', exact: true }).click();
  await section.getByLabel('金額', { exact: true }).fill('360000');
  await stage.selectOption('66848546');
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText('必須 あと 2 項目', { exact: false })).toBeVisible();
  expect(patches).toHaveLength(0);
  await dialog.screenshot({ path: 'docs/screenshots/job-copy-stage-move/02-required-fields.png' });
  await dialog.getByRole('spinbutton', { name: '応募数', exact: true }).fill('25');
  await dialog.getByRole('spinbutton', { name: '面接数', exact: true }).fill('10');
  await dialog.getByRole('button', { name: '移す', exact: true }).click();
  await expect(section.locator('.dc-green')).toHaveText('保存済み');
  await expect(stage).toHaveValue('66848546');
  await expect(section).toContainText('¥360,000');
  expect(patches).toHaveLength(1);
  expect(patches[0]).toEqual({ operation_id: expect.any(String), stage: { pipeline_id: '21596025', stage_id: '66848546' }, base: { oubo: '', mensetu: '', amount: '240000' }, set: { oubo: '25', mensetu: '10', amount: '360000' } });
  await expect.poll(() => companyReads).toBe(2);
  await section.screenshot({ path: 'docs/screenshots/job-copy-stage-move/03-saved.png', animations: 'disabled' });
  await page.getByRole('navigation', { name: '現在地' }).getByRole('button', { name: '案件管理' }).click();
  await expect(page.getByRole('heading', { name: 'ホーム', exact: true })).toBeVisible();
  await expect.poll(() => homeReads).toBeGreaterThan(initialHomeReads);
  await page.getByRole('group', { name: '顧客の絞り込み' }).getByRole('button', { name: /^すべて/ }).click();
  await expect(page.locator('.jh-latest')).toContainText('継続済');
  await expect(page.locator('.jh-customer-table tbody')).toContainText('0 ／ 全 1');
});
