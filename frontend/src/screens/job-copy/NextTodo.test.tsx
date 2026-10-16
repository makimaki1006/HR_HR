// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { defaultTodoDate, DealNextTodo, NextTodoDialog, todoRequest, validTodoDate } from './NextTodo';
import { buildPatchBody } from '../crm/writeModel';
import { contractDetail, contractSchema } from './dealContract.testFixtures';

beforeEach(() => {
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute('open', ''); };
  HTMLDialogElement.prototype.close = function () { this.removeAttribute('open'); };
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });
it('JSTの日付変更と7日後の既定日、読み取ったbaseと選択肢valueのPATCH本文', () => {
  expect(validTodoDate('2026-02-30')).toBe(false);
  expect(validTodoDate('2028-02-29')).toBe(true);
  expect(defaultTodoDate(new Date('2026-10-15T14:59:59Z'))).toBe('2026-10-22');
  expect(defaultTodoDate(new Date('2026-10-15T15:00:00Z'))).toBe('2026-10-23');
  expect(buildPatchBody(todoRequest('901', { actionyotei: '2026-10-01T00:00:00Z', actionnaiyou: 'old' }, '2026-10-23', 'call_value'), 'synthetic-op')).toEqual({ operation_id: 'synthetic-op', base: { actionyotei: '2026-10-01T00:00:00Z', actionnaiyou: 'old' }, set: { actionyotei: '2026-10-23', actionnaiyou: 'call_value' } });
});
function api(mode: 'saved' | 'queued' | 'conflict' | 'error') {
  const bodies: Record<string, unknown>[] = [];
  const fetcher = vi.fn((url: string, init?: RequestInit) => {
    let status = 200; let body: unknown = {};
    if (url.includes('edit-schema')) body = { ...contractSchema, editable: [{ object: 'deal', name: 'actionyotei', label: '予定日', type: 'date', field_type: 'date', options: [] }, { object: 'deal', name: 'actionnaiyou', label: '内容', type: 'enumeration', field_type: 'select', options: [{ label: '電話する', value: 'call_value' }] }] };
    else if (url.includes('workspace')) body = { ...contractDetail(), selected: { deal: { actionyotei: '2026-10-01', actionnaiyou: 'old' } } };
    else if (url.endsWith('/todo')) body = { status: 'ready', reasons: [] };
    else if (init?.method === 'PATCH') {
      const patch = JSON.parse(typeof init.body === 'string' ? init.body : '') as Record<string, unknown>; bodies.push(patch);
      if (mode === 'saved') body = { status: 'saved', operation_id: patch.operation_id, values: patch.set, objects_values: {} };
      if (mode === 'queued') { status = 202; body = { status: 'queued', operation_id: patch.operation_id }; }
      if (mode === 'conflict') { status = 409; body = { status: 'conflict', object: 'deal', current: { actionyotei: '2026-10-20', actionnaiyou: 'call_value' }, changed_by_hubspot: ['actionyotei'] }; }
      if (mode === 'error') { status = 403; body = { error: 'forbidden' }; }
    }
    return Promise.resolve(new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } }));
  });
  vi.stubGlobal('fetch', fetcher); return { bodies, fetcher };
}
async function save() {
  await screen.findByRole('option', { name: '電話する' });
  fireEvent.change(screen.getByLabelText('日付'), { target: { value: '2026-10-23' } });
  fireEvent.change(screen.getByLabelText('内容'), { target: { value: 'call_value' } });
  fireEvent.click(screen.getByRole('button', { name: /^保存$/ }));
}
it('小窓から2項目だけ保存し、緑の結果と保存後の一覧更新を通知する', async () => {
  const mock = api('saved'); const saved = vi.fn(); const event = vi.fn();
  window.addEventListener('job-copy-plan-saved', event);
  render(<NextTodoDialog dealId="901" onClose={vi.fn()} onSaved={saved} />);
  await save(); await screen.findByText('保存済み'); await waitFor(() => { expect(saved).toHaveBeenCalledTimes(1); });
  expect(mock.bodies).toHaveLength(1);
  expect(mock.bodies[0]).toMatchObject({ base: { actionyotei: '2026-10-01', actionnaiyou: 'old' }, set: { actionyotei: '2026-10-23', actionnaiyou: 'call_value' } });
  expect(Object.keys(mock.bodies[0] ?? {})).toEqual(['operation_id', 'base', 'set']);
  expect(event).toHaveBeenCalledTimes(1);
  expect(mock.fetcher.mock.calls.some(([url]) => url === '/api/job-copy/deals/901/refresh')).toBe(true);
  fireEvent.change(screen.getByLabelText('日付'), { target: { value: '2026/10/24' } });
  expect(screen.queryByText('保存済み')).toBeNull();
  window.removeEventListener('job-copy-plan-saved', event);
});
it('反映待ちを黄色で表示し、保存済みと扱わない', async () => {
  api('queued'); const saved = vi.fn(); render(<NextTodoDialog dealId="901" onClose={vi.fn()} onSaved={saved} />);
  await save(); expect((await screen.findByText('一時保存済み・HubSpotへの反映待ち')).className).toBe('nt-yellow'); expect(saved).not.toHaveBeenCalled();
});
it('競合の現在値を表示し、確認して保存するとその現在値をbaseにする', async () => {
  const mock = api('conflict'); render(<NextTodoDialog dealId="901" onClose={vi.fn()} />);
  await save(); await screen.findByText('日付：現在「2026/10/20」');
  expect(mock.bodies).toHaveLength(1);
  fireEvent.click(screen.getByRole('button', { name: '確認して入力した値で保存' }));
  await waitFor(() => { expect(mock.bodies).toHaveLength(2); });
  expect(mock.bodies[1]).toMatchObject({ base: { actionyotei: '2026-10-20', actionnaiyou: 'call_value' } });
  expect(mock.bodies[0]?.operation_id).not.toBe(mock.bodies[1]?.operation_id);
});
it('権限のエラーを赤で表示し、キャッシュ更新を通知しない', async () => {
  api('error'); const saved = vi.fn(); render(<NextTodoDialog dealId="901" onClose={vi.fn()} onSaved={saved} />);
  await save(); expect((await screen.findByRole('alert')).className).toBe('nt-red'); expect(saved).not.toHaveBeenCalled();
});

it('案件の要対応も取得が終われば定期通信を止める', async () => {
  vi.useFakeTimers();
  try {
    const mock = api('saved');
    render(<DealNextTodo dealId="901" onSaved={vi.fn()} />);
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });
    const reads = () => mock.fetcher.mock.calls.filter(([url]) => url.endsWith('/todo')).length;
    expect(reads()).toBe(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(3_600_000); });
    expect(reads()).toBe(1);
    cleanup();
  } finally { vi.useRealTimers(); }
});
