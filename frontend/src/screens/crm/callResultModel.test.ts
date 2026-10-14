import { describe, expect, it } from 'vitest';
import {
  BPO_PIPELINE_ID, DRAFT_STORAGE_KEY, RESULT_GROUPS, RESULT_PROPERTY_NAMES, appendMemo, clearDraftEntry, emptyResultDraft, isCalendarDate, isEmptyDraft,
  loadStore, markRecorded, memoDatePrefix, msUntilNextJstMidnight, nextUnrecorded, optionLabel, parseStore, putDraft, quickStagesOf, resultChanges, resultTeamOf,
  previousDateLabel, saveStore, shownValue, todayJst, withResultProps,
} from './callResultModel';
import type { ResultFieldSpec } from './callResultModel';
import { MOC_DEAL_PROPERTIES } from './mocProperties';
import type { FieldDef } from './writeModel';

const defs = MOC_DEAL_PROPERTIES;
const TODAY = '2026-10-14';

describe('組 (チームごとの項目)', () => {
  it('BPO と新規営業の項目 (この順)', () => {
    expect(RESULT_GROUPS.bpo.fields.map(f => f.name)).toEqual(['risuto_kadennbi', 'bpo_13', 'bpo_14', 'bpo_16', 'bpo_51', 'nitteigiri']);
    expect(RESULT_GROUPS.sales.fields.map(f => f.name)).toEqual(['risuto_kadennbi', 'risuto_saikadennbi', 'saikadenn_zikan', 'risuto_bikou']);
    expect(RESULT_GROUPS.bpo.fields.filter(f => f.role === 'memo').map(f => f.name)).toEqual(['bpo_16']);
    expect(RESULT_GROUPS.sales.fields.filter(f => f.role === 'memo').map(f => f.name)).toEqual(['risuto_bikou']);
    expect(RESULT_GROUPS.bpo.fields.filter(f => !f.primary).map(f => f.name)).toEqual(['bpo_51', 'nitteigiri']);
    // 試作の項目・自動で付く項目は入れない
    for (const n of ['bpo_40', 'bpo_42', 'bpo_45', 'bpo_8', 'bpo_5', 'zenkaisuteji', 'bpo_54', 'hs_call_body', 'dealstage']) expect(RESULT_PROPERTY_NAMES).not.toContain(n);
    expect(RESULT_PROPERTY_NAMES).toEqual(['risuto_kadennbi', 'bpo_13', 'bpo_14', 'bpo_16', 'bpo_51', 'nitteigiri', 'risuto_saikadennbi', 'saikadenn_zikan', 'risuto_bikou']);
  });
  it('チームで決め、両方・どちらでもないときはパイプラインで決める', () => {
    expect(resultTeamOf(['BPO_リクロジ'], 'default')).toBe('bpo');
    expect(resultTeamOf(['新規営業'], BPO_PIPELINE_ID)).toBe('sales');
    expect(resultTeamOf(['BPO_リクロジ', '新規営業'], BPO_PIPELINE_ID)).toBe('bpo');
    expect(resultTeamOf(['BPO_リクロジ', '新規営業'], 'default')).toBe('sales');
    expect(resultTeamOf([], BPO_PIPELINE_ID)).toBe('bpo');
    expect(resultTeamOf(null, 'default')).toBe('sales');
    expect(resultTeamOf(['ほかのチーム'], null)).toBe('sales');
  });
  it('ステージのボタンは、よく使うもののうち今のパイプラインにあるもの (無ければステージが少ないときだけ全部)', () => {
    const bpo = [{ id: '1095387442', label: '未済' }, { id: '1095387443', label: '不通' }, { id: '1095387445', label: '不在' }];
    expect(quickStagesOf(RESULT_GROUPS.bpo, bpo).map(s => s.label)).toEqual(['不在', '不通']);
    const other = [{ id: 'a', label: 'A' }, { id: 'b', label: 'B' }];
    expect(quickStagesOf(RESULT_GROUPS.sales, other).map(s => s.id)).toEqual(['a', 'b']);
    expect(quickStagesOf(RESULT_GROUPS.sales, Array.from({ length: 9 }, (_, i) => ({ id: `s${String(i)}`, label: String(i) })))).toEqual([]);
  });
});

describe('追記のメモ', () => {
  it('複数行の項目は改行して、日付 (JST の MM/DD) を付けて末尾に足す', () => {
    expect(memoDatePrefix(TODAY)).toBe('10/14');
    expect(appendMemo('09/28 受付で不在\n10/02 再度不在', '午後に再架電', 'textarea', TODAY)).toBe('09/28 受付で不在\n10/02 再度不在\n10/14 午後に再架電');
    // 今の内容の末尾の空白・改行は詰める。足す文の前後の空白も詰め、中の改行は残す
    expect(appendMemo('前のメモ\n\n', '  1 行目\n2 行目 \n', 'textarea', TODAY)).toBe('前のメモ\n10/14 1 行目\n2 行目');
    expect(appendMemo(null, '最初のメモ', 'textarea', TODAY)).toBe('10/14 最初のメモ');
  });
  it('1 行の項目は「 / 」で足し、改行は空白にする', () => {
    expect(appendMemo('決裁は本社', '社長が決裁\n来週電話', 'text', TODAY)).toBe('決裁は本社 / 10/14 社長が決裁 来週電話');
    expect(appendMemo('', '最初', 'text', TODAY)).toBe('10/14 最初');
  });
  it('足すものが空なら変えない (null)', () => {
    expect(appendMemo('前のメモ', '   \n ', 'textarea', TODAY)).toBeNull();
    expect(appendMemo('前のメモ', '', 'text', TODAY)).toBeNull();
  });
});

describe('初期値と保存する変更', () => {
  const def = (name: string, kind: FieldDef['kind']): FieldDef => ({ object: 'deal', name, label: name, kind, options: [] });
  const spec = (name: string, role: ResultFieldSpec['role']): ResultFieldSpec => ({ name, role, primary: true });
  const fields = [
    { spec: spec('risuto_kadennbi', 'callDate'), def: def('risuto_kadennbi', 'date') },
    { spec: spec('bpo_13', 'nextDate'), def: def('bpo_13', 'date') },
    { spec: spec('bpo_16', 'memo'), def: def('bpo_16', 'textarea') },
    { spec: spec('nitteigiri', 'plain'), def: def('nitteigiri', 'number') },
  ];
  const at = (i: number) => { const f = fields[i]; if (!f) throw new Error(String(i)); return f; };
  it('架電日は前回の日が入っていても毎回今日。メモの欄は空 (足す文だけを書く)', () => {
    const d = emptyResultDraft();
    expect(shownValue(at(0).spec, at(0).def, d, { risuto_kadennbi: null }, TODAY)).toBe(TODAY);
    expect(shownValue(at(0).spec, at(0).def, d, { risuto_kadennbi: '2026-09-28' }, TODAY)).toBe(TODAY);
    expect(previousDateLabel('2026-09-28')).toBe('09/28');
    expect(previousDateLabel(null)).toBeNull();
    expect(previousDateLabel('')).toBeNull();
    expect(shownValue(at(2).spec, at(2).def, d, { bpo_16: '前のメモ' }, TODAY)).toBe('');
    // 利用者が空にした架電日は今日に戻さない
    expect(shownValue(at(0).spec, at(0).def, { stageId: '', values: { risuto_kadennbi: '' } }, { risuto_kadennbi: null }, TODAY)).toBe('');
  });
  it('変えた項目だけを、今の値 (base) と一緒に返す。メモは足した後の全文', () => {
    const current = { risuto_kadennbi: null, bpo_13: '2026-10-01', bpo_16: '前のメモ', nitteigiri: '1' };
    expect(resultChanges(fields, emptyResultDraft(), current, TODAY)).toEqual([
      { object: 'deal', name: 'risuto_kadennbi', base: null, value: TODAY },
    ]);
    const d = { stageId: '', values: { bpo_13: '2026-10-16', bpo_16: '午後に再架電', nitteigiri: '1' } };
    expect(resultChanges(fields, d, current, TODAY)).toEqual([
      { object: 'deal', name: 'risuto_kadennbi', base: null, value: TODAY },
      { object: 'deal', name: 'bpo_13', base: '2026-10-01', value: '2026-10-16' },
      { object: 'deal', name: 'bpo_16', base: '前のメモ', value: '前のメモ\n10/14 午後に再架電' },
    ]);
    // 消した値は null で送る
    expect(resultChanges(fields.slice(1, 2), { stageId: '', values: { bpo_13: '' } }, current, TODAY)).toEqual([{ object: 'deal', name: 'bpo_13', base: '2026-10-01', value: null }]);
  });
  it('案件の詳細を読む項目に組の項目を足す (上限を超える分は足さない)', () => {
    expect(withResultProps({ deals: ['dealname'], contacts: [], companies: [] }).deals).toEqual(['dealname', ...RESULT_PROPERTY_NAMES]);
    const same = { deals: [...RESULT_PROPERTY_NAMES], contacts: [], companies: [] };
    expect(withResultProps(same)).toBe(same);
    expect(withResultProps({ deals: ['a', 'b'], contacts: [], companies: [] }, 4).deals).toEqual(['a', 'b', 'risuto_kadennbi', 'bpo_13']);
  });
});

describe('todayJst / isCalendarDate', () => {
  it('the day changes at JST midnight, not UTC midnight', () => {
    expect(todayJst(Date.UTC(2026, 9, 7, 14, 59, 59))).toBe('2026-10-07'); // JST 23:59:59
    expect(todayJst(Date.UTC(2026, 9, 7, 15, 0, 0))).toBe('2026-10-08'); // JST 00:00
  });
  it('msUntilNextJstMidnight counts down to the next JST 00:00', () => {
    expect(msUntilNextJstMidnight(Date.UTC(2026, 9, 7, 14, 59, 59))).toBe(1000);
    expect(msUntilNextJstMidnight(Date.UTC(2026, 9, 7, 15, 0, 0))).toBe(86_400_000);
    expect(msUntilNextJstMidnight(Date.UTC(2026, 9, 8, 3, 0, 0))).toBe(12 * 3600_000);
  });
  it('accepts only real calendar dates in YYYY-MM-DD', () => {
    expect(isCalendarDate('2026-10-08')).toBe(true);
    expect(isCalendarDate('2028-02-29')).toBe(true);
    expect(isCalendarDate('2026-02-29')).toBe(false);
    expect(isCalendarDate('2026/10/08')).toBe(false);
  });
});

describe('optionLabel', () => {
  it('maps stored values to HubSpot display labels, including hidden options, falling back to the value', () => {
    expect(optionLabel(defs.bpo_10, '使われておりません')).toBe('現在使われておりません');
    expect(optionLabel(defs.bpo_10, '未知の値')).toBe('未知の値');
    expect(optionLabel(undefined, 'x')).toBe('x');
  });
});

describe('下書きの保存', () => {
  it('put / clear / markRecorded (保存したら下書きを消して印を付ける)', () => {
    let s = putDraft({ drafts: {}, recorded: {} }, 'live:1', { stageId: '1095387445', values: { bpo_16: '途中' } });
    expect(s.drafts['live:1']).toEqual({ stageId: '1095387445', values: { bpo_16: '途中' } });
    s = putDraft(s, 'live:1', emptyResultDraft());
    expect(s.drafts).toEqual({});
    s = putDraft(s, 'live:2', { stageId: '', values: { nitteigiri: '1' } });
    s = markRecorded(s, 'live:2');
    expect(s).toEqual({ drafts: {}, recorded: { 'live:2': true } });
    s = putDraft(s, 'live:3', { stageId: 'x', values: {} });
    expect(clearDraftEntry(s, 'live:3')).toEqual({ drafts: {}, recorded: { 'live:2': true } });
    expect(isEmptyDraft(emptyResultDraft())).toBe(true);
  });
  it('parseStore は形の正しい下書きだけを、保存した本人にだけ戻す', () => {
    const raw = JSON.stringify({
      user: 'A@example.invalid',
      drafts: {
        ok: { stageId: '1', values: { bpo_16: 'メモ' } },
        badName: { stageId: '', values: { 'x y': 'z' } },
        badValue: { stageId: '', values: { bpo_16: 3 } },
        old: { outcome: 'connected', memo: '試作の下書き' },
      },
      recorded: { ok: true, no: 'yes' },
    });
    expect(parseStore(raw, 'a@example.invalid')).toEqual({ drafts: { ok: { stageId: '1', values: { bpo_16: 'メモ' } } }, recorded: { ok: true } });
    expect(parseStore(raw, 'b@example.invalid')).toEqual({ drafts: {}, recorded: {} });
    expect(parseStore('{', 'a@example.invalid')).toEqual({ drafts: {}, recorded: {} });
  });
  it('load / save は使えない保存先でも落ちない', () => {
    const broken = { getItem: () => { throw new Error('x'); }, setItem: () => { throw new Error('x'); } };
    expect(loadStore(broken, 'a')).toEqual({ drafts: {}, recorded: {} });
    expect(saveStore(broken, { drafts: {}, recorded: {} }, 'a')).toBe(false);
    expect(saveStore(null, { drafts: {}, recorded: {} }, 'a')).toBe(false);
    const mem = new Map<string, string>();
    const ok = { getItem: (k: string) => mem.get(k) ?? null, setItem: (k: string, v: string) => { mem.set(k, v); } };
    expect(saveStore(ok, { drafts: { k: { stageId: '', values: { bpo_16: 'x' } } }, recorded: {} }, 'a')).toBe(true);
    expect(mem.has(DRAFT_STORAGE_KEY)).toBe(true);
    expect(loadStore(ok, 'a').drafts.k).toEqual({ stageId: '', values: { bpo_16: 'x' } });
  });
});

describe('nextUnrecorded', () => {
  const rec = (ids: string[]) => (id: string) => ids.includes(id);
  it('the next unrecorded row after the current one, wrapping to the top', () => {
    expect(nextUnrecorded(['1', '2', '3', '4'], '1', rec(['2']))).toBe('3');
    expect(nextUnrecorded(['1', '2', '3', '4'], '4', rec([]))).toBe('1');
    expect(nextUnrecorded(['1', '2', '3'], '2', rec(['1', '3']))).toBeNull();
  });
});
