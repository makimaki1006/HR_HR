import { expect, test } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
import { contractCatalog, contractDetail, contractSchema, contractValues } from '../../frontend/src/screens/job-copy/dealContract.testFixtures';

import { demoHome } from '../../frontend/src/screens/job-copy/demoHome';

const shots = 'docs/screenshots/job-copy-contract';
test('案件画面で契約を保存して緑、競合では現在値を確認して選ぶ', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1100 });
  await page.addInitScript(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
  let values = { ...contractValues };
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
  await expect(section).toContainText('240000');
  await expect(section).toContainText('230000');
  await section.getByRole('button', { name: '編集', exact: true }).click();
  await section.getByLabel('金額', { exact: true }).fill('360000');
  await section.getByLabel('契約プラン', { exact: true }).selectOption('premium');
  await section.getByLabel('契約開始日', { exact: true }).fill('2026-11-01');
  await section.getByLabel('応募数', { exact: true }).fill('25');
  await section.getByRole('button', { name: '保存', exact: true }).click();
  await expect(section.locator('.dc-green')).toHaveCount(4);
  await expect(section).toContainText('360000');
  await expect(section).toContainText('合成・充実');
  expect(patches[0].base).toEqual({ amount: '240000', contract_plan: 'basic', contract_start_date: '2026-10-01T00:00:00Z', oubo: '20' });
  expect(patches[0].set).toEqual({ amount: '360000', contract_plan: 'premium', contract_start_date: '2026-11-01', oubo: '25' });
  expect(patches[0].operation_id).toMatch(/^[a-zA-Z0-9_-]{8,64}$/);
  await expect.poll(() => detailReads).toBe(2);
  await mkdir(shots, { recursive: true });
  await section.screenshot({ path: `${shots}/01-saved.png` });
  conflict = true;
  await section.getByRole('button', { name: '編集', exact: true }).click();
  await section.getByLabel('金額', { exact: true }).fill('480000');
  await section.screenshot({ path: `${shots}/02-editing.png` });
  await section.getByRole('button', { name: '保存', exact: true }).click();
  await expect(section.getByRole('alert')).toContainText('現在の値「420000」／入力した値「480000」');
  expect(patches).toHaveLength(2);
  expect(patches[1].base).toEqual({ amount: '360000' });
  await section.screenshot({ path: `${shots}/03-conflict.png` });
  await section.getByRole('button', { name: '現在の値を使う', exact: true }).click();
  await expect(section).toContainText('420000');
  expect(patches).toHaveLength(2);
  await page.getByRole('navigation', { name: '現在地' }).getByRole('button', { name: '案件管理' }).click();
  await expect(page.getByRole('heading', { name: 'ホーム', exact: true })).toBeVisible();
  await expect.poll(() => homeReads).toBeGreaterThan(initialHomeReads);
});
