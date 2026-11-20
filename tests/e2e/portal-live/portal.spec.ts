import { readFileSync } from 'node:fs';
import { expect, test } from '@playwright/test';

const headers = { Origin: 'http://localhost:9230', 'X-Requested-With': 'fetch' };

test.beforeEach(async ({ context }) => {
  const file = process.env.PORTAL_E2E_STATE;
  if (!file) throw new Error('開発用のログイン状態がありません');
  const state = JSON.parse(readFileSync(file, 'utf8'));
  await context.addCookies(state.cookies);
});

test('Rust の応募者 API で保存し、再読込しても残り、古い行の保存は競合になる', async ({ page }) => {
  const api = page.request;
  const sheets = await (await api.get('/api/portal/sheets')).json();
  expect(sheets.sheets).toHaveLength(2);
  const id = sheets.sheets[0].id as string;
  const before = await (await api.get(`/api/portal/sheets/${id}/rows`)).json();
  expect(before.columns.some((c: { id: string }) => c.id === '社内メモ')).toBe(false);
  const column = before.columns.find((c: { label: string }) => c.label === '備考');
  expect(column.writable).toBe(true);
  const row = before.applicants[0];
  const url = `/api/portal/sheets/${id}/rows/${row.row}`;
  const value = '=これは数式として実行しない（動作確認）';
  const saved = await api.patch(url, { headers, data: { revision: row.revision, changes: [{ column: column.id, value }] } });
  expect(saved.status()).toBe(200);
  const response = await saved.json();
  const index = before.columns.indexOf(column);
  expect(response.applicant.cells[index]).toBe(value);
  const reread = await (await api.get(`/api/portal/sheets/${id}/rows`)).json();
  expect(reread.applicants.find((r: { row: number }) => r.row === row.row).cells[index]).toBe(value);
  const conflict = await api.patch(url, { headers, data: { revision: row.revision, changes: [{ column: column.id, value: '古い画面からの上書き' }] } });
  expect(conflict.status()).toBe(409);
  expect((await conflict.json()).latest.cells[index]).toBe(value);
  const denied = await api.patch(url, { data: { revision: response.applicant.revision, changes: [{ column: column.id, value: 'CSRFなし' }] } });
  expect(denied.status()).toBe(403);
});

test('PC の一覧から保存でき、別の画面での変更は上書きせず入力を残す', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto('/portal');
  await expect(page.getByText('応募者の管理', { exact: true })).toBeVisible();
  await expect(page.locator(':text-is("佐藤 一郎"):visible').first()).toBeVisible();
  await expect(page.getByText('サンプル介護サービス', { exact: true })).toHaveCount(0);
  await expect(page.getByText('社内だけのメモ', { exact: true })).toHaveCount(0);
  await page.screenshot({ path: '/private/tmp/hr-hr-portal-desktop.png', fullPage: true });
  await page.getByRole('button', { name: '佐藤 一郎さんの詳細を開く', exact: true }).click();
  let detail = page.getByRole('dialog', { name: '佐藤 一郎さんの詳細', exact: true });
  await detail.getByRole('group', { name: '備考', exact: true }).getByRole('textbox').fill('画面から保存したメモ');
  await detail.getByRole('button', { name: '変更を保存', exact: true }).click();
  await expect(detail.getByText('保存しました', { exact: true })).toBeVisible();
  await page.reload();
  detail = page.getByRole('dialog', { name: '佐藤 一郎さんの詳細', exact: true });
  const input = detail.getByRole('group', { name: '備考', exact: true }).getByRole('textbox');
  await expect(input).toHaveValue('画面から保存したメモ');
  await input.fill('この画面で入力していたメモ');
  const data = await (await page.request.get('/api/portal/sheets')).json();
  const id = data.sheets[0].id as string;
  const latest = await (await page.request.get(`/api/portal/sheets/${id}/rows`)).json();
  const nameIndex = latest.columns.findIndex((c: { id: string }) => c.id === '応募者名');
  const row = latest.applicants.find((r: { cells: string[] }) => r.cells[nameIndex] === '佐藤 一郎');
  const otherSave = await page.request.patch(`/api/portal/sheets/${id}/rows/${row.row}`, { headers, data: { revision: row.revision, changes: [{ column: '備考', value: '別の画面で先に保存したメモ' }] } });
  expect(otherSave.status()).toBe(200);
  await detail.getByRole('button', { name: '変更を保存', exact: true }).click();
  await expect(detail.getByText('シートで変更されています', { exact: true })).toBeVisible();
  await expect(input).toHaveValue('この画面で入力していたメモ');
});

test('スマホに応募者を表示し、ログアウト後は応募者の内容を表示しない', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/portal');
  await expect(page.locator(':text-is("佐藤 一郎"):visible').first()).toBeVisible();
  await page.screenshot({ path: '/private/tmp/hr-hr-portal-mobile.png', fullPage: true });
  await page.getByRole('button', { name: 'ログアウト', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'ログイン', exact: true })).toBeVisible();
  await expect(page.getByText('佐藤 一郎', { exact: true })).toHaveCount(0);
});
