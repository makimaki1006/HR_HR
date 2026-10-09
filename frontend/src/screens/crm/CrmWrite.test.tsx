// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { useMemo } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiHttpError } from '../../api/client';
import { classifyPatch } from './crmWrite';
import type { PatchOutcome, WriteApi } from './crmWrite';
import { StageMover } from './StageMove';
import { ConflictDialog, EditableValue } from './WriteWidgets';
import { PropertyPanel } from './PropertyPanel';
import { catalogIndex } from './propertyModel';
import type { SelectedProps } from './propertyModel';
import { FIXTURE_CATALOG } from './usePropertyCatalog';
import { invalidMessage, partialRequest, pollDelayMs, useCrmWrite } from './useCrmWrite';
import { useWriteBindings } from './writeBindings';
import { fixtureDetail } from './workspaceFixture';
import { kindOf } from './writeModel';
import type { EditSchema, EditableProp, OperationStatus } from './writeTypes';

afterEach(() => { cleanup(); vi.useRealTimers(); });

const DEAL = 'f-5';
const index = catalogIndex(FIXTURE_CATALOG);
const BPO10 = FIXTURE_CATALOG.objects[0]?.groups.flatMap(g => g.properties).find(p => p.name === 'bpo_10');
const BPO10_OPTIONS = (BPO10?.options ?? []).map(o => ({ value: o.value, label: o.label }));
const OPT = BPO10_OPTIONS[1] ?? { value: '', label: '' };

const ed = (name: string, label: string, type: string, field_type: string, options: EditableProp['options'] = []): EditableProp =>
  ({ object: 'deal', name, label, type, field_type, options });

const SELECTION: SelectedProps = {
  deals: ['dealname', 'bpo_32', 'risuto_bikou', 'syokusyu_risuto', 'kyotenkessaiari', 'amount', 'bpo_10', 'aposyutokubi'], contacts: [], companies: [],
};
const BIKOU = 'risuto_bikou';
const bikouLabel = index.deals.get(BIKOU)?.prop.label ?? '';
const bikouRaw = fixtureDetail(DEAL, SELECTION)?.selected.deal[BIKOU] ?? null;

function schemaOf(over: Partial<EditSchema> = {}): EditSchema {
  return {
    deal_id: DEAL, pipeline_id: '753186575', stage_id: '1095387446',
    editable: [
      ed(BIKOU, bikouLabel, 'string', 'textarea'), ed('syokusyu_risuto', '募集職種（リストデータ）', 'string', 'text'),
      ed('kyotenkessaiari', '拠点決済アリ', 'bool', 'booleancheckbox'), ed('amount', '金額', 'number', 'number'),
      ed('bpo_10', index.deals.get('bpo_10')?.prop.label ?? '', 'enumeration', 'select', BPO10_OPTIONS), ed('aposyutokubi', 'アポ取得日', 'date', 'date'),
    ],
    stages: [{ id: 'st-appo', label: 'アポ日確定', required: ['aposyutokubi'], shown: ['aposyutokubi', BIKOU] }],
    pipelines: [
      { id: '753186575', label: 'パイプラインA', stages: [{ id: '1095387446', label: '担当者ブロック' }, { id: 'st-appo', label: 'アポ日確定' }, { id: 'st-plain', label: '日程確保' }] },
      { id: 'pl-b', label: 'パイプラインB', stages: [{ id: 'b-new', label: '新規' }] },
    ],
    writes_enabled: true,
    ...over,
  };
}

type Patch = ReturnType<typeof vi.fn<WriteApi['patchDeal']>>;
function mockApi(schema: EditSchema, patch: Patch, operation?: WriteApi['operation']): WriteApi {
  return {
    editSchema: () => Promise.resolve({ ok: true as const, data: schema }),
    patchDeal: patch,
    operation: operation ?? (() => Promise.resolve({ ok: true as const, data: { operation_id: 'x', status: 'saved', attempts: 1, last_error_code: null, next_retry_at: null } satisfies OperationStatus })),
  };
}
const savedOut = (values: Record<string, string | null>): PatchOutcome => ({ kind: 'saved', response: { status: 'saved', values, objects_values: {}, fetched_at: '2026-10-09T00:00:00Z' } });

let ids = 0;
function Harness({ api, onSaved, pollMs = 5_000 }: { api: WriteApi; onSaved: (id: string) => void; pollMs?: number }) {
  const data = useMemo(() => fixtureDetail(DEAL, SELECTION), []);
  const write = useCrmWrite({ dealId: DEAL, api, onSaved, pollIntervalMs: pollMs, newId: () => `op-${String(++ids)}` });
  const b = useWriteBindings({
    write, dealId: DEAL, data, index, ownerNames: new Map(),
    loadValues: (_id, defs) => Promise.resolve(Object.fromEntries(defs.map(d => [d.name, d.name === BIKOU ? 'メモ原文' : null]))),
  });
  return <>
    {b.conflict !== null && <ConflictDialog view={b.conflict} />}
    <StageMover ctx={b.stage} fallback={<span>固定表示</span>} />
    <PropertyPanel catalog={{ phase: 'ready', catalog: FIXTURE_CATALOG, index }} onReloadCatalog={() => undefined} selection={SELECTION}
      onApply={() => undefined} data={b.view} placeholder="" ownerNames={new Map()} hasSelection write={b.panel} />
  </>;
}

async function setup(schema: EditSchema, patch: Patch, opts: { operation?: WriteApi['operation']; pollMs?: number } = {}) {
  const onSaved = vi.fn();
  render(<Harness api={mockApi(schema, patch, opts.operation)} onSaved={onSaved} {...(opts.pollMs !== undefined ? { pollMs: opts.pollMs } : {})} />);
  await act(async () => { await Promise.resolve(); await Promise.resolve(); });
  for (const t of screen.queryAllByRole('button', { expanded: false })) if (t.className.includes('pp-card-toggle')) fireEvent.click(t);
  return onSaved;
}
const body = (patch: Patch, call = 0) => patch.mock.calls[call]?.[1];
const editBtn = (label: string) => screen.getByRole('button', { name: `${label}を編集` });

describe('編集できる項目とできない項目', () => {
  it('editable の一覧にある項目だけに「編集」が付く (案件名・URL_求人検索は読み取りのまま)', async () => {
    await setup(schemaOf(), vi.fn());
    expect(editBtn(bikouLabel)).toBeTruthy();
    expect(screen.queryByRole('button', { name: '案件名を編集' })).toBeNull();
    expect(screen.queryByRole('button', { name: /URL_求人検索.*を編集/ })).toBeNull();
    expect(screen.getAllByRole('button', { name: /を編集$/ }).length).toBe(6);
  });

  it('editable_all のときは read_only 以外の表示中の項目すべてに付く', async () => {
    await setup(schemaOf({ editable: [], editable_all: true, read_only: ['bpo_32'] }), vi.fn());
    expect(editBtn('案件名')).toBeTruthy();
    expect(editBtn(bikouLabel)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /URL_求人検索.*を編集/ })).toBeNull();
  });

  it('writes_enabled=false のときは編集の入口が無く、試験運用中の注記が出る', async () => {
    await setup(schemaOf({ writes_enabled: false }), vi.fn());
    expect(screen.queryAllByRole('button', { name: /を編集$/ })).toHaveLength(0);
    expect(screen.getByTestId('writes-off-note').textContent).toBe('この案件はまだ編集できません（試験運用中）');
    expect(screen.queryByRole('combobox', { name: 'ステージを変更' })).toBeNull();
    expect(screen.getByText('固定表示')).toBeTruthy();
  });
});

describe('項目の保存', () => {
  it('編集を始めた時点の値が base になる (編集中に表示が変わっても変えない)', () => {
    const onSave = vi.fn();
    const def = { object: 'deal' as const, name: 'syokusyu_risuto', label: '募集職種', kind: 'text' as const, options: [] };
    const { rerender } = render(<EditableValue def={def} raw="旧" display="旧" status={undefined} onSave={onSave} />);
    fireEvent.click(screen.getByRole('button', { name: '募集職種を編集' }));
    rerender(<EditableValue def={def} raw="別の人が変えた値" display="別の人が変えた値" status={undefined} onSave={onSave} />);
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種' }), { target: { value: '新' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    expect(onSave).toHaveBeenCalledWith('旧', '新');
  });

  it('Esc で取りやめ (送らない)、Enter で保存、変えていなければ送らない', () => {
    const onSave = vi.fn();
    const def = { object: 'deal' as const, name: 'n', label: '項目', kind: 'text' as const, options: [] };
    render(<EditableValue def={def} raw="a" display="a" status={undefined} onSave={onSave} />);
    fireEvent.click(screen.getByRole('button', { name: '項目を編集' }));
    fireEvent.change(screen.getByRole('textbox', { name: '項目' }), { target: { value: 'b' } });
    fireEvent.keyDown(screen.getByRole('textbox', { name: '項目' }), { key: 'Escape' });
    expect(onSave).not.toHaveBeenCalled();
    expect(screen.queryByRole('textbox', { name: '項目' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '項目を編集' }));
    fireEvent.keyDown(screen.getByRole('textbox', { name: '項目' }), { key: 'Enter' });
    expect(onSave).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: '項目を編集' }));
    fireEvent.change(screen.getByRole('textbox', { name: '項目' }), { target: { value: 'c' } });
    fireEvent.keyDown(screen.getByRole('textbox', { name: '項目' }), { key: 'Enter' });
    expect(onSave).toHaveBeenCalledExactlyOnceWith('a', 'c');
  });

  it('選択式は表示名を見せて、HubSpot の値 (value) を送る', async () => {
    const patch: Patch = vi.fn(() => Promise.resolve(savedOut({ bpo_10: OPT.value })));
    await setup(schemaOf(), patch);
    expect(OPT.label).not.toBe(OPT.value);
    fireEvent.click(editBtn(index.deals.get('bpo_10')?.prop.label ?? ''));
    const select = screen.getByRole('combobox', { name: index.deals.get('bpo_10')?.prop.label ?? '' });
    expect(within(select).getByRole('option', { name: OPT.label })).toBeTruthy();
    fireEvent.change(select, { target: { value: OPT.value } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => { expect(patch).toHaveBeenCalled(); });
    expect(body(patch)?.set).toEqual({ bpo_10: OPT.value });
    expect(body(patch)?.base).toEqual({ bpo_10: fixtureDetail(DEAL, SELECTION)?.selected.deal.bpo_10 ?? null });
  });

  it('日付・数値・真偽の入力欄で、HubSpot の文字列で送る', async () => {
    const patch: Patch = vi.fn(() => Promise.resolve(savedOut({})));
    await setup(schemaOf(), patch);
    fireEvent.click(editBtn('アポ取得日'));
    const date = screen.getByLabelText('アポ取得日');
    expect(date.getAttribute('type')).toBe('date');
    fireEvent.change(date, { target: { value: '2026-10-20' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    expect(body(patch)?.set).toEqual({ aposyutokubi: '2026-10-20' });

    fireEvent.click(editBtn('金額'));
    expect(screen.getByLabelText('金額').getAttribute('type')).toBe('number');
    fireEvent.change(screen.getByLabelText('金額'), { target: { value: '150000' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(2); });
    expect(body(patch, 1)?.set).toEqual({ amount: '150000' });

    fireEvent.click(editBtn('拠点決済アリ'));
    const box = screen.getByLabelText('拠点決済アリ');
    expect(box.getAttribute('type')).toBe('checkbox');
    fireEvent.click(box);
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(3); });
    expect(body(patch, 2)?.set).toEqual({ kyotenkessaiari: 'true' });
  });

  it('複数行の項目は textarea で編集できる', async () => {
    await setup(schemaOf(), vi.fn());
    fireEvent.click(editBtn(bikouLabel));
    expect(screen.getByRole('textbox', { name: bikouLabel }).tagName).toBe('TEXTAREA');
    expect(screen.getByRole<HTMLTextAreaElement>('textbox', { name: bikouLabel }).value).toBe(bikouRaw);
  });

  it('200 → 緑の「保存済み」、新しい値を表示し、詳細を読み直す', async () => {
    const patch: Patch = vi.fn(() => Promise.resolve(savedOut({ syokusyu_risuto: 'サーバーが返した値' })));
    const onSaved = await setup(schemaOf(), patch);
    fireEvent.click(editBtn('募集職種（リストデータ）'));
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: '入力した値' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    const ok = await screen.findByText('✓ 保存済み');
    expect(ok.getAttribute('data-state')).toBe('saved');
    expect(screen.getByText('サーバーが返した値')).toBeTruthy();
    expect(onSaved).toHaveBeenCalledWith(DEAL);
    expect(body(patch)?.base).toEqual({ syokusyu_risuto: 'ドライバー(架空)' });
  });

  it('422 → 項目の下にエラーを出し、保存済みとは言わない', async () => {
    const patch: Patch = vi.fn(() => Promise.resolve<PatchOutcome>({ kind: 'invalid', body: { status: 'invalid', errors: { syokusyu_risuto: '255 文字までです' }, missing_required: [] } }));
    const onSaved = await setup(schemaOf(), patch);
    fireEvent.click(editBtn('募集職種（リストデータ）'));
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: 'x' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    expect((await screen.findByText(/255 文字までです/)).getAttribute('data-state')).toBe('error');
    expect(screen.queryByText('✓ 保存済み')).toBeNull();
    expect(onSaved).not.toHaveBeenCalled();
    expect(screen.getByText('ドライバー(架空)')).toBeTruthy();
  });

  it('PATCH の本文に HubSpot の内部名は画面に出ない (表示は表示名のみ)', async () => {
    await setup(schemaOf(), vi.fn());
    const text = document.body.textContent;
    expect(text).not.toContain('syokusyu_risuto');
    expect(text).not.toContain('bpo_10');
  });

  it('403 writes_disabled → 編集の入口を閉じて注記を出す', async () => {
    const patch: Patch = vi.fn(() => Promise.resolve<PatchOutcome>({ kind: 'writes_disabled' }));
    await setup(schemaOf(), patch);
    fireEvent.click(editBtn('募集職種（リストデータ）'));
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: 'x' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await screen.findByTestId('writes-off-note');
    expect(screen.queryAllByRole('button', { name: /を編集$/ })).toHaveLength(0);
  });
});

describe('反映待ち (202)', () => {
  it('黄色の反映待ちを出し、5 秒後・その 10 秒後に確認して、保存済みになったら緑にする', async () => {
    vi.useFakeTimers();
    const patch: Patch = vi.fn(() => Promise.resolve<PatchOutcome>({ kind: 'queued', response: { status: 'queued', operation_id: 'op-q' } }));
    let state: OperationStatus['status'] = 'pending';
    const operation = vi.fn(() => Promise.resolve({ ok: true as const, data: { operation_id: 'op-q', status: state, attempts: 1, last_error_code: null, next_retry_at: null } }));
    const onSaved = await setup(schemaOf(), patch, { operation });
    fireEvent.click(editBtn('募集職種（リストデータ）'));
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: '待ち中の値' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    const yellow = screen.getByText(/反映待ち/);
    expect(yellow.getAttribute('data-state')).toBe('queued');
    expect(screen.queryByText('✓ 保存済み')).toBeNull();
    expect(screen.getByText('待ち中の値')).toBeTruthy();
    expect(onSaved).not.toHaveBeenCalled();
    await act(async () => { await vi.advanceTimersByTimeAsync(4_999); });
    expect(operation).toHaveBeenCalledTimes(0);
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(operation).toHaveBeenCalledTimes(1);
    expect(operation).toHaveBeenCalledWith('op-q');
    expect(screen.getByText(/反映待ち/).getAttribute('data-state')).toBe('queued');
    state = 'saved';
    await act(async () => { await vi.advanceTimersByTimeAsync(9_999); });
    expect(operation).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(operation).toHaveBeenCalledTimes(2);
    expect(screen.getByText('✓ 保存済み').getAttribute('data-state')).toBe('saved');
    expect(onSaved).toHaveBeenCalledWith(DEAL);
  });

  it('確認の間隔は 5, 10, 20, 40 秒と倍々に延び、最大 60 秒', async () => {
    expect([0, 1, 2, 3, 4, 5, 6, 100].map(n => pollDelayMs(n))).toEqual([5_000, 10_000, 20_000, 40_000, 60_000, 60_000, 60_000, 60_000]);
    vi.useFakeTimers();
    const patch: Patch = vi.fn(() => Promise.resolve<PatchOutcome>({ kind: 'queued', response: { status: 'queued', operation_id: 'op-b' } }));
    const operation = vi.fn(() => Promise.resolve({ ok: true as const, data: { operation_id: 'op-b', status: 'pending' as const, attempts: 1, last_error_code: null, next_retry_at: null } }));
    await setup(schemaOf(), patch, { operation });
    fireEvent.click(editBtn('募集職種（リストデータ）'));
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: 'v' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    let expected = 0;
    for (const gap of [5_000, 10_000, 20_000, 40_000, 60_000, 60_000]) {
      await act(async () => { await vi.advanceTimersByTimeAsync(gap - 1); });
      expect(operation).toHaveBeenCalledTimes(expected);
      await act(async () => { await vi.advanceTimersByTimeAsync(1); });
      expected += 1;
      expect(operation).toHaveBeenCalledTimes(expected);
    }
  });

  it('30 分たっても保存済みにならなければ「まだ反映待ちです」を出して黄色のまま、確認をやめる (33 回)', async () => {
    vi.useFakeTimers();
    const patch: Patch = vi.fn(() => Promise.resolve<PatchOutcome>({ kind: 'queued', response: { status: 'queued', operation_id: 'op-slow' } }));
    const operation = vi.fn(() => Promise.resolve({ ok: true as const, data: { operation_id: 'op-slow', status: 'retrying' as const, attempts: 3, last_error_code: null, next_retry_at: null } }));
    await setup(schemaOf(), patch, { operation });
    fireEvent.click(editBtn('募集職種（リストデータ）'));
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: 'v' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await act(async () => { await vi.advanceTimersByTimeAsync(31 * 60_000); });
    // 5 + 10 + 20 + 40 秒の 4 回のあとは 60 秒おき。待ちの合計が 30 分 (1,800 秒) に届く 33 回目 (1,815 秒) でやめる
    expect(operation).toHaveBeenCalledTimes(33);
    const slow = screen.getByText(/まだ反映待ちです/);
    expect(slow.getAttribute('data-state')).toBe('queued');
    await act(async () => { await vi.advanceTimersByTimeAsync(5 * 60_000); });
    expect(operation).toHaveBeenCalledTimes(33);
  });

  describe('タブが隠れているとき', () => {
    const setVisibility = (v: 'visible' | 'hidden') => {
      Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => v });
      document.dispatchEvent(new Event('visibilitychange'));
    };
    afterEach(() => { Reflect.deleteProperty(document, 'visibilityState'); });

    it('隠れている間は確認せず、時間も数えず、表示に戻ったらすぐ確認する', async () => {
      vi.useFakeTimers();
      const patch: Patch = vi.fn(() => Promise.resolve<PatchOutcome>({ kind: 'queued', response: { status: 'queued', operation_id: 'op-h' } }));
      let state: OperationStatus['status'] = 'pending';
      const operation = vi.fn(() => Promise.resolve({ ok: true as const, data: { operation_id: 'op-h', status: state, attempts: 1, last_error_code: null, next_retry_at: null } }));
      const onSaved = await setup(schemaOf(), patch, { operation });
      fireEvent.click(editBtn('募集職種（リストデータ）'));
      fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: 'v' } });
      fireEvent.click(screen.getByRole('button', { name: '保存' }));
      await act(async () => { await vi.advanceTimersByTimeAsync(0); });
      setVisibility('hidden');
      // 2 時間隠れていても、確認の通信は 1 回も出ない
      await act(async () => { await vi.advanceTimersByTimeAsync(2 * 60 * 60_000); });
      expect(operation).toHaveBeenCalledTimes(0);
      expect(screen.getByText(/反映待ち/).getAttribute('data-state')).toBe('queued');
      state = 'saved';
      setVisibility('visible');
      await act(async () => { await vi.advanceTimersByTimeAsync(0); });
      expect(operation).toHaveBeenCalledTimes(1);
      expect(screen.getByText('✓ 保存済み').getAttribute('data-state')).toBe('saved');
      expect(onSaved).toHaveBeenCalledWith(DEAL);
    });

    it('隠れていた時間は 30 分に数えない (戻ったあとも確認を続ける)', async () => {
      vi.useFakeTimers();
      const patch: Patch = vi.fn(() => Promise.resolve<PatchOutcome>({ kind: 'queued', response: { status: 'queued', operation_id: 'op-h2' } }));
      const operation = vi.fn(() => Promise.resolve({ ok: true as const, data: { operation_id: 'op-h2', status: 'pending' as const, attempts: 1, last_error_code: null, next_retry_at: null } }));
      await setup(schemaOf(), patch, { operation });
      fireEvent.click(editBtn('募集職種（リストデータ）'));
      fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: 'v' } });
      fireEvent.click(screen.getByRole('button', { name: '保存' }));
      await act(async () => { await vi.advanceTimersByTimeAsync(0); });
      setVisibility('hidden');
      await act(async () => { await vi.advanceTimersByTimeAsync(3 * 60 * 60_000); });
      setVisibility('visible');
      await act(async () => { await vi.advanceTimersByTimeAsync(0); });
      expect(operation).toHaveBeenCalledTimes(1);
      expect(screen.queryByText(/まだ反映待ちです/)).toBeNull();
      await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
      expect(operation).toHaveBeenCalledTimes(2);
    });
  });

  it('failed になったら赤で知らせる', async () => {
    vi.useFakeTimers();
    const patch: Patch = vi.fn(() => Promise.resolve<PatchOutcome>({ kind: 'queued', response: { status: 'queued', operation_id: 'op-f' } }));
    const operation = vi.fn(() => Promise.resolve({ ok: true as const, data: { operation_id: 'op-f', status: 'failed' as const, attempts: 5, last_error_code: null, next_retry_at: null } }));
    await setup(schemaOf(), patch, { operation });
    fireEvent.click(editBtn('募集職種（リストデータ）'));
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: 'v' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    expect(screen.getByText(/HubSpot に反映できませんでした/).getAttribute('data-state')).toBe('error');
    expect(screen.queryByText('✓ 保存済み')).toBeNull();
  });
});

describe('衝突 (409)', () => {
  const conflictPatch = (): Patch => vi.fn()
    .mockResolvedValueOnce({ kind: 'conflict', body: { status: 'conflict', object: 'deal', current: { syokusyu_risuto: 'HubSpotで変わった値' }, changed_by_hubspot: ['syokusyu_risuto'] } })
    .mockResolvedValue(savedOut({ syokusyu_risuto: '自分の値' }));
  async function startConflict(patch: Patch) {
    const onSaved = await setup(schemaOf(), patch);
    fireEvent.click(editBtn('募集職種（リストデータ）'));
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: '自分の値' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    const dlg = await screen.findByRole('dialog');
    return { dlg, onSaved };
  }

  it('HubSpot の今の値と自分の値を並べ、「HubSpotの値を使う」なら送り直さずその値を表示する', async () => {
    const patch = conflictPatch();
    const { dlg, onSaved } = await startConflict(patch);
    expect(within(dlg).getByText('HubSpotで変わった値')).toBeTruthy();
    expect(within(dlg).getByText('自分の値')).toBeTruthy();
    fireEvent.click(within(dlg).getByRole('button', { name: 'HubSpotの値を使う' }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(patch).toHaveBeenCalledTimes(1);
    expect(screen.getByText('HubSpotで変わった値')).toBeTruthy();
    expect(onSaved).toHaveBeenCalledWith(DEAL);
  });

  it('「自分の値で上書き」なら base を HubSpot の今の値にして、新しい operation_id で送り直す', async () => {
    const patch = conflictPatch();
    const { dlg } = await startConflict(patch);
    fireEvent.click(within(dlg).getByRole('button', { name: '自分の値で上書き' }));
    await screen.findByText('✓ 保存済み');
    expect(patch).toHaveBeenCalledTimes(2);
    expect(body(patch, 1)?.base).toEqual({ syokusyu_risuto: 'HubSpotで変わった値' });
    expect(body(patch, 1)?.set).toEqual({ syokusyu_risuto: '自分の値' });
    expect(body(patch, 1)?.operation_id).not.toBe(body(patch, 0)?.operation_id);
  });
});

describe('operation_id', () => {
  it('操作ごとに別の ID。通信の失敗で再試行するときは同じ ID を使う', async () => {
    const patch: Patch = vi.fn()
      .mockResolvedValueOnce({ kind: 'error', message: '通信できませんでした。' })
      .mockResolvedValue(savedOut({}));
    await setup(schemaOf(), patch);
    fireEvent.click(editBtn('募集職種（リストデータ）'));
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: '一回目' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    const err = await screen.findByText(/保存できていません/);
    expect(err.getAttribute('data-state')).toBe('error');
    expect(screen.queryByText('✓ 保存済み')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'もう一度保存' }));
    await screen.findByText('✓ 保存済み');
    expect(body(patch, 1)?.operation_id).toBe(body(patch, 0)?.operation_id);
    // 次の編集は別の操作
    fireEvent.click(editBtn('募集職種（リストデータ）'));
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: '二回目' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(3); });
    expect(body(patch, 2)?.operation_id).not.toBe(body(patch, 0)?.operation_id);
  });
});

describe('operation_id (失敗の後の再試行)', () => {
  it('受け付けられなかった (queue_full) 後の再試行は、新しい operation_id で送る (古い失敗の結果が返り続けない)', async () => {
    const patch: Patch = vi.fn()
      .mockResolvedValueOnce({ kind: 'queue_full' })
      .mockResolvedValue(savedOut({}));
    await setup(schemaOf(), patch);
    fireEvent.click(editBtn('募集職種（リストデータ）'));
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: '一回目' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await screen.findByText(/何も保存されていません/);
    fireEvent.click(screen.getByRole('button', { name: 'もう一度保存' }));
    await screen.findByText('✓ 保存済み');
    expect(body(patch, 1)?.operation_id).not.toBe(body(patch, 0)?.operation_id);
  });

  it('429 rate_limited は赤いメッセージを出し、再送待ちにせず、新しい operation_id で送り直せる', async () => {
    const patch: Patch = vi.fn()
      .mockResolvedValueOnce({ kind: 'rate_limited' })
      .mockResolvedValue(savedOut({}));
    await setup(schemaOf(), patch);
    fireEvent.click(editBtn('募集職種（リストデータ）'));
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: '一回目' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    const msg = await screen.findByText(/短時間に保存が多すぎます。少し待ってから保存してください。何も保存されていません。/);
    expect(msg.getAttribute('role')).toBe('alert');
    expect(msg.className).toContain('wr-error');
    expect(screen.queryByText(/同期待ち|再送待ち/)).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'もう一度保存' }));
    await screen.findByText('✓ 保存済み');
    expect(body(patch, 1)?.operation_id).not.toBe(body(patch, 0)?.operation_id);
  });

  it('管理者が破棄した操作 (410) は新しい operation_id で送り直せる', async () => {
    const patch: Patch = vi.fn()
      .mockResolvedValueOnce({ kind: 'discarded' })
      .mockResolvedValue(savedOut({}));
    await setup(schemaOf(), patch);
    fireEvent.click(editBtn('募集職種（リストデータ）'));
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: '一回目' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await screen.findByText(/取り消しました/);
    fireEvent.click(screen.getByRole('button', { name: 'もう一度保存' }));
    await screen.findByText('✓ 保存済み');
    expect(body(patch, 1)?.operation_id).not.toBe(body(patch, 0)?.operation_id);
  });
});

describe('途中まで書けた保存 (partial)', () => {
  it('案件は書けて担当者が拒否された → 案件の項目は保存済みと表示し、詳細を読み直す (「何も保存されていない」と言わない)', async () => {
    const patch: Patch = vi.fn(() => Promise.resolve<PatchOutcome>({
      kind: 'invalid',
      body: { status: 'invalid', errors: { '_': 'HubSpot が値を受け付けませんでした' }, missing_required: [], partial: { values: { syokusyu_risuto: '入力した値' }, objects_values: {} } },
    }));
    const onSaved = await setup(schemaOf(), patch);
    fireEvent.click(editBtn('募集職種（リストデータ）'));
    fireEvent.change(screen.getByRole('textbox', { name: '募集職種（リストデータ）' }), { target: { value: '入力した値' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    expect((await screen.findByText('✓ 保存済み')).getAttribute('data-state')).toBe('saved');
    expect(onSaved).toHaveBeenCalledWith(DEAL);
  });

  it('項目のエラーのキー: 案件は項目名、担当者・会社は contact.x / company.x、全体は _ (どれも画面に出る)', () => {
    expect(invalidMessage({ 'contact.jobtitle': '長すぎます' }, 'contact', 'jobtitle', false)).toBe('長すぎます');
    expect(invalidMessage({ 'company.domain': '不正' }, 'company', 'domain', false)).toBe('不正');
    expect(invalidMessage({ amount: '数値で' }, 'deal', 'amount', false)).toBe('数値で');
    expect(invalidMessage({ _: 'HubSpot が値を受け付けませんでした' }, 'contact', 'jobtitle', false)).toBe('HubSpot が値を受け付けませんでした');
    expect(invalidMessage({ 'contact._': '担当者の一括エラー' }, 'contact', 'jobtitle', false)).toBe('担当者の一括エラー');
    // 他の項目に固有のエラーがあるときは、その文言を関係ない項目に出さない
    expect(invalidMessage({ amount: '数値で' }, 'deal', 'dealname', false)).toBe('保存できませんでした。入力を確かめてください。');
    // どの項目にも固有のエラーが無いときだけ、残りのエラーを代わりに出す
    expect(invalidMessage({ operation_id: 'operation_id が不正です' }, 'deal', 'amount', true)).toBe('operation_id が不正です');
  });

  it('partial から、書けたオブジェクトの変更だけを取り出す', () => {
    const req = {
      dealId: DEAL, recordIds: { contact: 'c1', company: null },
      changes: [
        { object: 'deal' as const, name: 'amount', base: '1', value: '2' },
        { object: 'contact' as const, name: 'jobtitle', base: 'a', value: 'b' },
      ],
    };
    const onlyDeal = partialRequest(req, { values: { amount: '2' }, objects_values: {} });
    expect(onlyDeal.changes.map(c => c.name)).toEqual(['amount']);
    const onlyContact = partialRequest(req, { values: {}, objects_values: { contact: { jobtitle: 'b' } } });
    expect(onlyContact.changes.map(c => c.name)).toEqual(['jobtitle']);
  });
});

describe('ステージの変更', () => {
  const stageSelect = () => screen.getByRole('combobox', { name: 'ステージを変更' });

  it('必須項目のあるステージ → 項目の入力画面。必須が埋まるまで「移す」は押せず、stage と set を 1 回で送る', async () => {
    const patch: Patch = vi.fn(() => Promise.resolve(savedOut({})));
    const onSaved = await setup(schemaOf(), patch);
    fireEvent.change(stageSelect(), { target: { value: 'st-appo' } });
    const dlg = await screen.findByRole('dialog');
    expect(within(dlg).getByText('このステージに移すには次の項目が必要です')).toBeTruthy();
    expect(within(dlg).getByText(/アポ取得日/).textContent).toContain('(必須)');
    // 現在の値が入った状態で開く
    await waitFor(() => { expect(within(dlg).getByLabelText<HTMLTextAreaElement>(new RegExp(bikouLabel.slice(0, 6))).value).toBe(bikouRaw); });
    const move = within(dlg).getByRole('button', { name: '移す' });
    expect((move as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(within(dlg).getByLabelText(/アポ取得日/), { target: { value: '2026-11-01' } });
    expect((move as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(move);
    await waitFor(() => { expect(screen.queryByRole('dialog')).toBeNull(); });
    expect(patch).toHaveBeenCalledTimes(1);
    const b = body(patch);
    expect(b?.stage).toEqual({ pipeline_id: '753186575', stage_id: 'st-appo' });
    expect(b?.set).toEqual({ aposyutokubi: '2026-11-01' });
    expect(b?.base).toEqual({ aposyutokubi: null });
    expect(onSaved).toHaveBeenCalledWith(DEAL);
    expect(screen.getAllByText('✓ 保存済み').length).toBe(2);
  });

  it('規則の無いステージ → 確認だけで直接移す (set は空)', async () => {
    const patch: Patch = vi.fn(() => Promise.resolve(savedOut({})));
    await setup(schemaOf(), patch);
    fireEvent.change(stageSelect(), { target: { value: 'st-plain' } });
    const dlg = await screen.findByRole('dialog');
    expect(within(dlg).getByText(/「日程確保」に移します/)).toBeTruthy();
    expect(patch).not.toHaveBeenCalled();
    fireEvent.click(within(dlg).getByRole('button', { name: '移す' }));
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    expect(body(patch)?.stage).toEqual({ pipeline_id: '753186575', stage_id: 'st-plain' });
    expect(body(patch)?.set).toEqual({});
  });

  it('確認で「やめる」なら送らない', async () => {
    const patch: Patch = vi.fn();
    await setup(schemaOf(), patch);
    fireEvent.change(stageSelect(), { target: { value: 'st-plain' } });
    fireEvent.click(within(await screen.findByRole('dialog')).getByRole('button', { name: 'やめる' }));
    expect(patch).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('別のパイプラインへ: パイプラインとステージを選び、そのパイプラインの ID で送る', async () => {
    const patch: Patch = vi.fn(() => Promise.resolve(savedOut({})));
    await setup(schemaOf(), patch);
    fireEvent.click(screen.getByRole('button', { name: '別のパイプラインへ移す' }));
    const dlg = await screen.findByTestId('pipeline-modal');
    const next = within(dlg).getByRole('button', { name: '次へ' });
    expect((next as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(within(dlg).getByLabelText('パイプライン'), { target: { value: 'pl-b' } });
    fireEvent.change(within(dlg).getByLabelText('ステージ'), { target: { value: 'b-new' } });
    fireEvent.click(next);
    const confirm = await screen.findByTestId('stage-modal');
    fireEvent.click(within(confirm).getByRole('button', { name: '移す' }));
    await waitFor(() => { expect(patch).toHaveBeenCalledTimes(1); });
    expect(body(patch)?.stage).toEqual({ pipeline_id: 'pl-b', stage_id: 'b-new' });
  });

  it('422 (必須の不足) → モーダルを開いたまま不足の項目名を出す', async () => {
    const patch: Patch = vi.fn(() => Promise.resolve<PatchOutcome>({ kind: 'invalid', body: { status: 'invalid', errors: {}, missing_required: ['aposyutokubi'] } }));
    await setup(schemaOf(), patch);
    fireEvent.change(stageSelect(), { target: { value: 'st-appo' } });
    const dlg = await screen.findByRole('dialog');
    await waitFor(() => { expect(within(dlg).getByLabelText(/アポ取得日/)).toBeTruthy(); });
    fireEvent.change(within(dlg).getByLabelText(/アポ取得日/), { target: { value: '2026-11-01' } });
    fireEvent.click(within(dlg).getByRole('button', { name: '移す' }));
    expect((await within(dlg).findByText(/次の項目を入力してください: アポ取得日/)).getAttribute('role')).toBe('alert');
    expect(screen.getByRole('dialog')).toBeTruthy();
  });
});

describe('契約の読み取り', () => {
  it('PATCH の結果を分類する', () => {
    const http = (status: number, b: unknown) => classifyPatch({ ok: false, error: new ApiHttpError(status, b) });
    expect(classifyPatch({ ok: true, data: { status: 'queued', operation_id: 'o' } }).kind).toBe('queued');
    expect(http(409, { status: 'conflict', object: 'contact', current: {}, changed_by_hubspot: [] }).kind).toBe('conflict');
    expect(http(422, { status: 'invalid', errors: {}, missing_required: [] }).kind).toBe('invalid');
    expect(http(403, { error: 'writes_disabled' }).kind).toBe('writes_disabled');
    expect(http(403, { error: 'forbidden' }).kind).toBe('forbidden');
    expect(http(503, { error: 'queue_full' }).kind).toBe('queue_full');
    expect(http(503, { error: 'queue_unavailable', error_kind: 'queue_unavailable' }).kind).toBe('queue_full');
    // 送った後で台帳を更新できなかった: 保存できたか分からないので「何も保存されていない」にしない
    expect(http(503, { error: 'queue_uncertain', error_kind: 'queue_uncertain' }).kind).toBe('error');
    expect(http(410, { error: 'discarded' }).kind).toBe('discarded');
    expect(http(429, { error: 'rate_limited' }).kind).toBe('rate_limited');
    expect(http(500, undefined).kind).toBe('error');
    const withPartial = http(403, { error: 'forbidden', partial: { values: { amount: '2' }, objects_values: {} } });
    expect(withPartial.kind === 'forbidden' && withPartial.partial?.values).toEqual({ amount: '2' });
    const err = http(404, { error: 'not_found', partial: { values: { amount: '2' }, objects_values: { contact: {} } } });
    expect(err.kind === 'error' && err.partial?.objects_values).toEqual({ contact: {} });
  });

  it('項目の種類 → 入力欄', () => {
    expect(kindOf('string', 'textarea', 0)).toBe('textarea');
    expect(kindOf('enumeration', 'select', 2)).toBe('select');
    expect(kindOf('enumeration', 'select', 0)).toBeNull();
    expect(kindOf('enumeration', 'checkbox', 2)).toBe('multi');
    expect(kindOf('datetime', 'date', 0)).toBeNull();
    expect(kindOf('bool', 'booleancheckbox', 2)).toBe('bool');
  });
});
