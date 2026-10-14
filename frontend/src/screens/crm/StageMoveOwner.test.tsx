// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { useMemo } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { PatchOutcome, WriteApi } from './crmWrite';
import { FIXTURE_OWNERS } from './ownerModel';
import { OwnerChoicesProvider } from './OwnerSelect';
import type { OwnerChoices } from './OwnerSelect';
import { catalogIndex } from './propertyModel';
import { StageMover, sectionsOf } from './StageMove';
import { FIXTURE_CATALOG } from './usePropertyCatalog';
import { useCrmWrite } from './useCrmWrite';
import { useWriteBindings } from './writeBindings';
import { fixtureDetail } from './workspaceFixture';
import type { EditSchema, EditableProp, OperationStatus } from './writeTypes';

afterEach(() => { cleanup(); });

const DEAL = 'f-5';
const index = catalogIndex(FIXTURE_CATALOG);
const ed = (name: string, label: string, type: string, field_type: string, owner_ref = false): EditableProp =>
  ({ object: 'deal', name, label, type, field_type, options: [], owner_ref });

const SCHEMA: EditSchema = {
  deal_id: DEAL, pipeline_id: '753186575', stage_id: 'st-0',
  editable: [
    ed('bpo_26', 'アポ獲得者', 'enumeration', 'select', true), ed('fx_appo_date', 'アポ日', 'date', 'date'),
    ed('fx_appo_company', 'アポ先の会社名', 'string', 'text'), ed('fx_hear_budget', '採用予算(万円)', 'number', 'number'),
    ed('fx_hear_next', '次回の連絡日', 'date', 'date'),
    ed('fstantousha', 'FS担当者', 'enumeration', 'select', true), ed('consultant', 'コンサル担当', 'enumeration', 'select', true),
  ],
  stages: [{
    id: 'st-appo', label: 'アポ日確定',
    required: ['bpo_26', 'fx_appo_date', 'fx_appo_company', 'fx_hear_budget'],
    shown: ['fx_appo_company', 'fx_appo_date', 'bpo_26', 'fx_hear_budget', 'fx_hear_next', 'fstantousha'],
  }],
  pipelines: [{ id: '753186575', label: 'パイプラインA', stages: [{ id: 'st-0', label: '未済' }, { id: 'st-appo', label: 'アポ日確定' }] }],
  writes_enabled: true,
};

type Patch = ReturnType<typeof vi.fn<WriteApi['patchDeal']>>;
const savedOut = (): PatchOutcome => ({ kind: 'saved', response: { status: 'saved', values: {}, objects_values: {}, fetched_at: '2026-10-09T00:00:00Z' } });

let ids = 0;
function Harness({ patch, meId }: { patch: Patch; meId: string | null }) {
  const api = useMemo<WriteApi>(() => ({
    editSchema: () => Promise.resolve({ ok: true as const, data: SCHEMA }),
    patchDeal: patch,
    operation: () => Promise.resolve({ ok: true as const, data: { operation_id: 'x', status: 'saved', attempts: 1, last_error_code: null, next_retry_at: null } satisfies OperationStatus }),
  }), [patch]);
  const data = useMemo(() => fixtureDetail(DEAL, { deals: ['dealname'], contacts: [], companies: [] }), []);
  const write = useCrmWrite({ dealId: DEAL, api, onSaved: () => undefined, pollIntervalMs: 5_000, newId: () => `op-${String(++ids)}` });
  const b = useWriteBindings({ write, dealId: DEAL, data, index, ownerNames: new Map(), meOwnerId: meId, loadValues: (_id, defs) => Promise.resolve(Object.fromEntries(defs.map(d => [d.name, null]))) });
  const choices = useMemo<OwnerChoices>(() => ({ state: { phase: 'ready', owners: FIXTURE_OWNERS, truncated: false, meId }, meId, reload: () => undefined }), [meId]);
  return <OwnerChoicesProvider value={choices}><StageMover ctx={b.stage} fallback={<span>固定表示</span>} /></OwnerChoicesProvider>;
}

async function openModal(patch: Patch, meId: string | null = '9001') {
  render(<Harness patch={patch} meId={meId} />);
  await act(async () => { await Promise.resolve(); await Promise.resolve(); });
  fireEvent.change(screen.getByRole('combobox', { name: 'ステージを変更' }), { target: { value: 'st-appo' } });
  const dlg = await screen.findByRole('dialog');
  await within(dlg).findByRole('combobox', { name: 'アポ獲得者' });
  return dlg;
}
/** ラベルに一致する最初の入力欄 (日付欄はカレンダーのボタンも同じ名前に当たるので、先頭だけ使う) */
const first = (dlg: HTMLElement, name: RegExp): HTMLElement => within(dlg).getAllByLabelText(name)[0] ?? dlg;
const ownerBox = (dlg: HTMLElement, label: string) => within(dlg).getByRole<HTMLInputElement>('combobox', { name: label });

describe('担当者を選ぶ型の項目 (ステージを移す画面)', () => {
  it('担当者の select に名前が出る。名前で絞り込め、選ぶと値は owner ID で送る', async () => {
    const patch: Patch = vi.fn(() => Promise.resolve(savedOut()));
    const dlg = await openModal(patch, null);
    expect(ownerBox(dlg, 'アポ獲得者').value).toBe('');
    fireEvent.focus(ownerBox(dlg, 'アポ獲得者'));
    const list = within(dlg).getByRole('listbox');
    expect(within(list).getAllByRole('option').map(o => o.textContent)).toContain('サンプル 二郎(jiro@example.invalid)');
    expect(within(list).queryByText(/ダミー 退職/)).toBeNull(); // 退職者は、選ばれていなければ出さない
    fireEvent.change(ownerBox(dlg, 'アポ獲得者'), { target: { value: 'jiro' } });
    expect(within(within(dlg).getByRole('listbox')).getAllByRole('option').map(o => o.textContent)).toEqual(['(未入力)', 'サンプル 二郎(jiro@example.invalid)']);
    fireEvent.mouseDown(within(dlg).getByRole('option', { name: /サンプル 二郎/ }));
    expect(ownerBox(dlg, 'アポ獲得者').value).toBe('サンプル 二郎');
    fireEvent.change(first(dlg, /アポ日/), { target: { value: '2026-11-01' } });
    fireEvent.change(first(dlg, /アポ先の会社名/), { target: { value: '架空商事' } });
    fireEvent.change(first(dlg, /採用予算/), { target: { value: '120' } });
    fireEvent.click(within(dlg).getByRole('button', { name: '移す' }));
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    expect(patch.mock.calls[0]?.[1]?.set).toMatchObject({ bpo_26: '9002' });
  });

  it('アポ獲得者が空なら本人が最初から入る。FS担当者には入れない', async () => {
    const patch: Patch = vi.fn(() => Promise.resolve(savedOut()));
    const dlg = await openModal(patch, '9001');
    expect(ownerBox(dlg, 'アポ獲得者').value).toBe('架空 一郎');
    expect(ownerBox(dlg, 'FS担当者').value).toBe('');
    // 必須はアポ獲得者が埋まっているので、残りは 3 項目
    expect(within(dlg).getByTestId('stage-remaining').textContent).toContain('必須 あと 3 項目');
    fireEvent.change(first(dlg, /アポ日/), { target: { value: '2026-11-01' } });
    fireEvent.change(first(dlg, /アポ先の会社名/), { target: { value: '架空商事' } });
    fireEvent.change(first(dlg, /採用予算/), { target: { value: '120' } });
    fireEvent.click(within(dlg).getByRole('button', { name: '移す' }));
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    const set = patch.mock.calls[0]?.[1]?.set;
    expect(set?.bpo_26).toBe('9001');
    expect(set).not.toHaveProperty('fstantousha');
  });

  it('自分で「未入力」に戻した後は、本人を入れ直さない', async () => {
    const dlg = await openModal(vi.fn(), '9001');
    expect(ownerBox(dlg, 'アポ獲得者').value).toBe('架空 一郎');
    fireEvent.focus(ownerBox(dlg, 'アポ獲得者'));
    fireEvent.mouseDown(within(dlg).getByRole('option', { name: '(未入力)' }));
    expect(ownerBox(dlg, 'アポ獲得者').value).toBe('');
    expect(within(dlg).getByTestId('stage-remaining').textContent).toContain('必須 あと 4 項目');
  });

  it('本人の担当者 ID を引けないときは空のまま', async () => {
    const patch: Patch = vi.fn();
    const dlg = await openModal(patch, null);
    expect(ownerBox(dlg, 'アポ獲得者').value).toBe('');
    expect(within(dlg).getByTestId('stage-remaining').textContent).toContain('必須 あと 4 項目');
  });

  it('必須の残り数が減っていき、0 になると「すべて入力済み」。未入力の行は印が付く', async () => {
    const patch: Patch = vi.fn();
    const dlg = await openModal(patch, null);
    const rem = () => within(dlg).getByTestId('stage-remaining').textContent;
    expect(rem()).toContain('必須 あと 4 項目');
    const row = (name: string) => dlg.querySelector<HTMLElement>(`[data-field="${name}"]`);
    expect(row('fx_appo_date')?.className).toContain('is-missing');
    expect(row('fx_hear_next')?.className).not.toContain('is-missing'); // 必須でない項目には付かない
    fireEvent.change(first(dlg, /アポ日/), { target: { value: '2026-11-01' } });
    expect(rem()).toContain('必須 あと 3 項目');
    expect(row('fx_appo_date')?.className).not.toContain('is-missing');
    fireEvent.change(first(dlg, /アポ先の会社名/), { target: { value: '架空商事' } });
    fireEvent.change(first(dlg, /採用予算/), { target: { value: '120' } });
    fireEvent.mouseDown((fireEvent.focus(ownerBox(dlg, 'アポ獲得者')), within(dlg).getByRole('option', { name: /架空 一郎\(ichiro@/ })));
    expect(rem()).toBe('必須項目はすべて入力済みです');
  });

  it('「必須 あと N 項目」を押すと、最初の未入力の項目にフォーカスが移る。「移す」を押しても同じで、送らない', async () => {
    const patch: Patch = vi.fn();
    const dlg = await openModal(patch, '9001');
    // 未入力の必須は、規則の並びで アポ先の会社名 → アポ日 → 採用予算
    fireEvent.click(within(dlg).getByTestId('stage-remaining'));
    expect(document.activeElement).toBe(first(dlg, /アポ先の会社名/));
    fireEvent.change(first(dlg, /アポ先の会社名/), { target: { value: '架空商事' } });
    fireEvent.click(within(dlg).getByRole('button', { name: '移す' }));
    expect(patch).not.toHaveBeenCalled();
    expect(document.activeElement?.getAttribute('aria-label')).toBe('アポ日');
  });

  it('項目はグループの見出しで区切る (項目の一覧にあるグループ名)。一覧に無ければ「その他の項目」', async () => {
    const dlg = await openModal(vi.fn(), '9001');
    const titles = within(dlg).getAllByRole('heading', { level: 4 }).map(h => h.textContent);
    expect(titles).toEqual(['アポ情報(架空)', 'ヒアリング内容(架空)', 'その他の項目']);
    expect(sectionsOf([], () => null)).toEqual([]);
  });

  it('422 で返った必須が画面に無いときは、内部名ではなく表示名で知らせる', async () => {
    const patch: Patch = vi.fn(() => Promise.resolve<PatchOutcome>({ kind: 'invalid', body: { status: 'invalid', errors: {}, missing_required: ['consultant', 'fx_appo_date'] } }));
    const dlg = await openModal(patch, '9001');
    fireEvent.change(first(dlg, /アポ日/), { target: { value: '2026-11-01' } });
    fireEvent.change(first(dlg, /アポ先の会社名/), { target: { value: '架空商事' } });
    fireEvent.change(first(dlg, /採用予算/), { target: { value: '120' } });
    fireEvent.click(within(dlg).getByRole('button', { name: '移す' }));
    const alert = await within(dlg).findByText(/この画面で入力できないものがあります/);
    expect(alert.textContent).toContain('(コンサル担当)');
    expect(alert.textContent).not.toContain('consultant');
    expect(alert.textContent).toContain('次の項目を入力してください: アポ日');
  });
});
