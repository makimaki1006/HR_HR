// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { ProposalMemo, proposalMemoRequest } from './ProposalMemo';
import { buildPatchBody } from '../crm/writeModel';
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });
it('空・既存のメモ・改行・JSTの日付の頭を共通関数で組み立てる', () => {
  const now = new Date('2026-10-18T15:00:00Z');
  expect(proposalMemoRequest('101', null, '最初の提案', now).changes[0]?.value).toBe('10/19 最初の提案');
  expect(proposalMemoRequest('101', '', '最初の提案', now).changes[0]?.value).toBe('10/19 最初の提案');
  expect(buildPatchBody(proposalMemoRequest('101', '前のメモ', '  求人を見直す\r\n 来週相談する  ', now), 'fixture-op')).toEqual({ operation_id: 'fixture-op', base: { teian: '前のメモ' }, set: { teian: '前のメモ / 10/19 求人を見直す 来週相談する' } });
  expect(proposalMemoRequest('101', null, '提案', new Date('2026-10-18T14:59:59Z')).changes[0]?.value).toBe('10/18 提案');
});
function api(mode: 'saved' | 'queued' | 'conflict' | 'off' | 'missing' | 'error') {
  const bodies: { operation_id: string; base: { teian: string }; set: { teian: string } }[] = [];
  let current = '前のメモ';
  vi.stubGlobal('fetch', vi.fn((url: string, init?: RequestInit) => {
    let status = 200; let body: unknown = {};
    if (url.includes('edit-schema')) body = { writes_enabled: mode !== 'off', editable: mode === 'missing' ? [] : [{ object: 'deal', name: 'teian', type: 'string' }] };
    else if (url.includes('workspace')) body = { selected: { deal: { teian: current } }, partial: [] };
    else if (init?.method === 'PATCH') {
      const patch = JSON.parse(typeof init.body === 'string' ? init.body : '') as typeof bodies[number]; bodies.push(patch);
      if (mode === 'conflict' && bodies.length === 1) { status = 409; current = 'ほかの人のメモ'; body = { status: 'conflict', object: 'deal', current: { teian: current }, changed_by_hubspot: ['teian'] }; }
      else if (mode === 'queued') { status = 202; body = { status: 'queued', operation_id: patch.operation_id }; }
      else if (mode === 'error') { status = 500; }
      else { current = patch.set.teian; body = { status: 'saved', operation_id: patch.operation_id, values: patch.set, objects_values: {} }; }
    }
    return Promise.resolve(new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } }));
  })); return bodies;
}
async function enter() {
  const input = await screen.findByLabelText<HTMLTextAreaElement>('追記するメモ');
  fireEvent.change(input, { target: { value: '来週相談する' } });
  fireEvent.click(screen.getByRole('button', { name: 'メモを保存' }));
  return input;
}
it('保存は既存PATCHを1回だけ送り、緑を表示する', async () => {
  const bodies = api('saved'); render(<ProposalMemo dealId="101" />);
  const input = await enter(); expect((await screen.findByText('保存済み')).className).toBe('nt-green');
  expect(bodies).toHaveLength(1); expect(bodies[0]?.set.teian).toMatch(/^前のメモ \/ \d{2}\/\d{2} 来週相談する$/); expect(input.value).toBe('');
});
it('競合で入力を残し、最新の内容への追記を改めて保存する', async () => {
  const bodies = api('conflict'); render(<ProposalMemo dealId="101" />);
  const input = await enter(); await screen.findByText('ほかの更新があります。最新のメモを確認してください。');
  expect(input.value).toBe('来週相談する'); expect(screen.getByText('ほかの人のメモ')).toBeTruthy(); expect(bodies).toHaveLength(1);
  fireEvent.click(screen.getByRole('button', { name: '最新のメモを確認' }));
  await waitFor(() => { expect(screen.queryByRole('alert')).toBeNull(); });
  expect(input.value).toBe('来週相談する');
  fireEvent.click(screen.getByRole('button', { name: 'メモを保存' })); await screen.findByText('保存済み');
  expect(bodies).toHaveLength(2); expect(bodies[1]?.base.teian).toBe('ほかの人のメモ'); expect(bodies[1]?.set.teian).toMatch(/^ほかの人のメモ \/ \d{2}\/\d{2} 来週相談する$/);
});
it.each(['off', 'missing'] as const)('書けないときは入力欄を隠して理由を表示する（%s）', async mode => {
  api(mode); render(<ProposalMemo dealId="101" />);
  await screen.findByText(mode === 'off' ? 'この案件はまだ編集できません（試験運用中）' : 'このメモを編集する権限がありません。');
  expect(screen.queryByRole('textbox')).toBeNull(); expect(screen.queryByRole('button', { name: 'メモを保存' })).toBeNull();
});
it('反映待ちは黄色、入力を空にして二重の追記を防ぐ', async () => {
  api('queued'); render(<ProposalMemo dealId="101" />); const input = await enter();
  expect((await screen.findByText('一時保存済み・HubSpotへの反映待ち')).className).toBe('nt-yellow'); expect(input.value).toBe(''); expect(input.disabled).toBe(true);
});
it('通信エラーは赤で入力を残し、再送は同じ操作を使う', async () => {
  const bodies = api('error'); render(<ProposalMemo dealId="101" />); const input = await enter();
  expect((await screen.findByRole('alert')).className).toBe('nt-red'); expect(input.value).toBe('来週相談する');
  fireEvent.click(screen.getByRole('button', { name: 'メモを保存' })); await waitFor(() => { expect(bodies).toHaveLength(2); });
  expect(bodies[1]).toEqual(bodies[0]);
});
