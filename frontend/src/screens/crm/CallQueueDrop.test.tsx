// @vitest-environment happy-dom
// 記録して条件から外れた行が、一覧から外れる (架空サンプルのモードで画面全体を通して確かめる)
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CallQueueScreen } from './CallQueueScreen';
import type { WriteApi } from './crmWrite';
import { fakeWriteApi, resetFakeWrite } from './fakeWrite';
import { okCatalogFetch, okUserFetch, resetDockStorage } from './queueTestUtil';
import type { PatchRequest } from './writeTypes';

// JST 2026-10-14 12:00
const NOW = () => Date.UTC(2026, 9, 14, 3, 0, 0);
const TODAY = '2026-10-14';

beforeEach(() => { try { window.sessionStorage.clear(); } catch { /* ignore */ } resetDockStorage(); resetFakeWrite(); });
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

type PatchSpy = ReturnType<typeof vi.fn<WriteApi['patchDeal']>>;
function spyApi(): { api: WriteApi; patch: PatchSpy } {
  const patch = vi.fn<WriteApi['patchDeal']>((id, body) => fakeWriteApi.patchDeal(id, body));
  return { api: { ...fakeWriteApi, patchDeal: patch }, patch };
}
async function renderFixture(search: string, api: WriteApi) {
  render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} writeApi={api} now={NOW}
    initialSearch={`?view=queue&mode=fixture${search}`} />);
  await waitFor(() => { expect(screen.getByRole('list', { name: '架電キュー' })).toBeTruthy(); });
}
const list = () => screen.getByRole('list', { name: '架電キュー' });
const form = () => screen.getByRole('form', { name: '架電結果の入力' });
const open = (company: string) => { fireEvent.click(within(list()).getByText(company)); fireEvent.click(screen.getByRole('tab', { name: '架電結果の入力' })); };
const recordBtn = () => within(form()).getByRole<HTMLButtonElement>('button', { name: /記録して次へ/ });
async function formReady() { await within(form()).findByRole('button', { name: /^不在/ }); }
const bodyOf = (patch: PatchSpy): PatchRequest => { const c = patch.mock.calls[0]; if (!c) throw new Error('no PATCH'); return c[1]; };
const cond = (...cs: { property: string; operator: string; values: string[] }[]) => `&conditions=${encodeURIComponent(JSON.stringify(cs))}`;
const names = () => Array.from(list().querySelectorAll('.cq-row-company')).map(e => e.textContent);
const selectedName = () => list().querySelector('.cq-row-button[aria-pressed="true"] .cq-row-company')?.textContent;
const count = () => screen.getByTestId('queue-count').textContent;

describe('記録して条件から外れた行は一覧から外れる', () => {
  it('「架電日が今日より前」で絞った一覧: 記録すると架電日が今日になるので、その行が消えて次の行が選ばれる', async () => {
    const { api, patch } = spyApi();
    await renderFixture(cond({ property: 'risuto_kadennbi', operator: 'BEFORE_TODAY', values: [] }), api);
    const before = names();
    const at = before.indexOf('架空食品株式会社');
    expect(at).toBeGreaterThanOrEqual(0);
    const nextName = before[at + 1];
    expect(nextName).toBeTruthy();
    open('架空食品株式会社');
    await formReady();
    fireEvent.click(recordBtn());
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    expect(bodyOf(patch).set.risuto_kadennbi).toBe(TODAY);
    await waitFor(() => { expect(names()).not.toContain('架空食品株式会社'); });
    // ほかの行は同じ順で残り、次の行が選ばれている (選択が崩れない)
    expect(names()).toEqual(before.filter(n => n !== '架空食品株式会社'));
    expect(selectedName()).toBe(nextName);
    expect(count()).toContain('(保存して外れた 1 件)');
    expect(count()).toContain(`${String(before.length - 1)} 件を表示`);
  });

  it('続けて 2 件記録すると、外れた件数は 2 件になる', async () => {
    const { api, patch } = spyApi();
    await renderFixture(cond({ property: 'risuto_kadennbi', operator: 'BEFORE_TODAY', values: [] }), api);
    const before = names();
    open(before[0] ?? '');
    await formReady();
    fireEvent.click(recordBtn());
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    await waitFor(() => { expect(count()).toContain('(保存して外れた 1 件)'); });
    await formReady();
    fireEvent.click(recordBtn());
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(2); });
    await waitFor(() => { expect(count()).toContain('(保存して外れた 2 件)'); });
    expect(names()).toEqual(before.slice(2));
    expect(selectedName()).toBe(before[2]);
  });

  it('書いた項目に関係しない条件では、記録しても行は残り、外れた件数も出ない', async () => {
    const { api, patch } = spyApi();
    await renderFixture(cond({ property: 'amount', operator: 'GTE', values: ['0'] }), api);
    const before = names();
    expect(before).toContain('架空食品株式会社');
    open('架空食品株式会社');
    await formReady();
    fireEvent.click(recordBtn());
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    await waitFor(() => { expect(selectedName()).not.toBe('架空食品株式会社'); });
    expect(names()).toEqual(before);
    expect(count()).not.toContain('外れた');
  });

  it('判定できない条件 (文字を含む) だけのときは、記録しても行は残る', async () => {
    const { api, patch } = spyApi();
    await renderFixture(cond({ property: 'dealname', operator: 'CONTAINS_TOKEN', values: ['採用支援'] }), api);
    const before = names();
    expect(before).toContain('架空食品株式会社');
    open('架空食品株式会社');
    await formReady();
    fireEvent.click(recordBtn());
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    await waitFor(() => { expect(selectedName()).not.toBe('架空食品株式会社'); });
    expect(names()).toEqual(before);
    expect(count()).not.toContain('外れた');
  });

  it('保存できなかった (赤) ときは、行は外れない', async () => {
    const { api } = spyApi();
    const failing: WriteApi = { ...api, patchDeal: () => Promise.resolve({ kind: 'error', message: '通信に失敗しました。' }) };
    await renderFixture(cond({ property: 'risuto_kadennbi', operator: 'BEFORE_TODAY', values: [] }), failing);
    const before = names();
    open('架空食品株式会社');
    await formReady();
    fireEvent.click(recordBtn());
    await within(form()).findByText(/保存できていません/);
    expect(names()).toEqual(before);
    expect(selectedName()).toBe('架空食品株式会社');
  });
});
