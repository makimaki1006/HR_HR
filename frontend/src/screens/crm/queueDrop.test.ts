import { describe, expect, it } from 'vitest';
import { leavesQueue } from './queueDrop';
import type { DropContext } from './queueDrop';
import { DEFAULT_FILTERS } from './queueModel';
import type { QueueFilters } from './queueModel';
import type { PropertyCondition } from './queuePropertyFilter';

const TODAY = '2026-10-15';
const KINDS: Record<string, string> = { risuto_kadennbi: 'date', bpo_13: 'date', bpo_20: 'date', memo: 'string', amount: 'number', lastmod: 'datetime', rank: 'enumeration' };
const ctx = (over: Partial<QueueFilters> = {}, extra: Partial<DropContext> = {}): DropContext => ({
  filters: { ...DEFAULT_FILTERS, ...over }, mode: 'live', today: TODAY, meOwnerId: '9001', kindOf: p => KINDS[p] ?? null, currentStageId: '1095387442', ...extra,
});
const c = (property: string, operator: string, ...values: string[]): PropertyCondition => ({ property, operator, values });

describe('leavesQueue: 記録・編集のあと、条件から外れたと言い切れるときだけ外す', () => {
  it('「架電日が今日より前」の条件で、架電日を今日にすると外れる / 昨日にしても残る', () => {
    const f = { conditions: [c('risuto_kadennbi', 'BEFORE_TODAY')] };
    expect(leavesQueue({ props: { risuto_kadennbi: TODAY } }, ctx(f))).toBe(true);
    expect(leavesQueue({ props: { risuto_kadennbi: '2026-10-14' } }, ctx(f))).toBe(false);
  });
  it('「架電日が空(本日未架電)」の条件で、架電日を入れると外れる / 空のままなら残る', () => {
    const f = { conditions: [c('risuto_kadennbi', 'NOT_HAS_PROPERTY')] };
    expect(leavesQueue({ props: { risuto_kadennbi: TODAY } }, ctx(f))).toBe(true);
    expect(leavesQueue({ props: { risuto_kadennbi: null } }, ctx(f))).toBe(false);
  });
  it('「次回架電日が今日以前」の条件で、次回架電日を未来にすると外れる / 今日以前なら残る', () => {
    const f = { conditions: [c('bpo_13', 'LTE', TODAY)] };
    expect(leavesQueue({ props: { bpo_13: '2026-10-20' } }, ctx(f))).toBe(true);
    expect(leavesQueue({ props: { bpo_13: '2026-10-15' } }, ctx(f))).toBe(false);
    expect(leavesQueue({ props: { bpo_13: null } }, ctx(f))).toBe(true);
  });
  it('まとまり同士は OR、まとまりの中は AND', () => {
    const a = [c('risuto_kadennbi', 'NOT_HAS_PROPERTY'), c('memo', 'EQ', 'x')];
    const b = [c('bpo_13', 'BEFORE_TODAY')];
    // a は架電日を入れて偽。b は書いた値 (bpo_13 を書いていない) で判定できない → 残す
    expect(leavesQueue({ props: { risuto_kadennbi: TODAY } }, ctx({ conditions: a, conditionGroups: [a, b] }))).toBe(false);
    // b も偽になれば外れる
    expect(leavesQueue({ props: { risuto_kadennbi: TODAY, bpo_13: TODAY } }, ctx({ conditions: a, conditionGroups: [a, b] }))).toBe(true);
    // AND の片方が偽なら、書いていない項目の条件があっても外れる
    expect(leavesQueue({ props: { risuto_kadennbi: TODAY } }, ctx({ conditions: a }))).toBe(true);
  });
  it('書いた項目に関係しない条件では残る', () => {
    const f = { conditions: [c('risuto_kadennbi', 'NOT_HAS_PROPERTY')] };
    expect(leavesQueue({ props: { memo: '話した' } }, ctx(f))).toBe(false);
    expect(leavesQueue({ props: { memo: '話した' } }, ctx())).toBe(false);
  });
  it('判定できない条件では残る: 文字を含む・日時の項目・型が分からない項目・形式が違う日付', () => {
    expect(leavesQueue({ props: { memo: 'abc' } }, ctx({ conditions: [c('memo', 'CONTAINS_TOKEN', 'zzz')] }))).toBe(false);
    expect(leavesQueue({ props: { lastmod: '2026-10-15T00:00:00Z' } }, ctx({ conditions: [c('lastmod', 'BEFORE_TODAY')] }))).toBe(false);
    expect(leavesQueue({ props: { unknown_prop: 'a' } }, ctx({ conditions: [c('unknown_prop', 'GTE', '5')] }))).toBe(false);
    expect(leavesQueue({ props: { risuto_kadennbi: '1760486400000' } }, ctx({ conditions: [c('risuto_kadennbi', 'BEFORE_TODAY')] }))).toBe(false);
    expect(leavesQueue({ props: { amount: null } }, ctx({ conditions: [c('amount', 'NEQ', '5')] }))).toBe(false);
  });
  it('選択肢・数値・真偽の条件', () => {
    expect(leavesQueue({ props: { rank: 'B' } }, ctx({ conditions: [c('rank', 'IN', 'A')] }))).toBe(true);
    expect(leavesQueue({ props: { rank: 'A' } }, ctx({ conditions: [c('rank', 'IN', 'A')] }))).toBe(false);
    expect(leavesQueue({ props: { amount: '100' } }, ctx({ conditions: [c('amount', 'GTE', '500')] }))).toBe(true);
    expect(leavesQueue({ props: { amount: '900' } }, ctx({ conditions: [c('amount', 'BETWEEN', '500', '1000')] }))).toBe(false);
  });
  it('ステージ: 絞り込んだステージ以外へ移すと外れる / 含まれるなら残る / 別のパイプラインへ移すと外れる', () => {
    const f = { stages: ['1095387442', '1095387443'] };
    expect(leavesQueue({ props: {}, stage: { pipeline_id: '753186575', stage_id: '1095387443' } }, ctx(f))).toBe(false);
    expect(leavesQueue({ props: {}, stage: { pipeline_id: '753186575', stage_id: 'zzz' } }, ctx(f))).toBe(true);
    expect(leavesQueue({ props: {}, stage: { pipeline_id: 'default', stage_id: 'appointmentscheduled' } }, ctx({ stages: [] }))).toBe(true);
  });
  it('ステージの条件 (dealstage の IN) も書いた値で判定する', () => {
    const f = { conditions: [c('dealstage', 'IN', '1095387442')] };
    expect(leavesQueue({ props: {}, stage: { pipeline_id: '753186575', stage_id: '1095387443' } }, ctx(f))).toBe(true);
  });
  it('架電対象では、架電禁止理由を入れると外れる。「すべての取引」では残る', () => {
    expect(leavesQueue({ props: { bpo_3: '断られた' } }, ctx())).toBe(true);
    expect(leavesQueue({ props: { bpo_3: '断られた' } }, ctx({ target: 'all' }))).toBe(false);
    expect(leavesQueue({ props: { bpo_3: null } }, ctx())).toBe(false);
  });
  it('「次回日が来たものだけ」では、次回架電日を未来にすると外れる', () => {
    expect(leavesQueue({ props: { bpo_13: '2026-10-30' } }, ctx({ due: 'today' }))).toBe(true);
    expect(leavesQueue({ props: { bpo_13: '2026-10-10' } }, ctx({ due: 'today' }))).toBe(false);
  });
  it('日付範囲・担当者の絞り込み', () => {
    expect(leavesQueue({ props: { bpo_20: TODAY } }, ctx({ lastTo: '2026-10-14' }))).toBe(true);
    expect(leavesQueue({ props: { bpo_20: '2026-10-01' } }, ctx({ lastTo: '2026-10-14' }))).toBe(false);
    expect(leavesQueue({ props: { hubspot_owner_id: '777' } }, ctx({ owner: 'me' }))).toBe(true);
    expect(leavesQueue({ props: { hubspot_owner_id: '9001' } }, ctx({ owner: 'me' }))).toBe(false);
    expect(leavesQueue({ props: { hubspot_owner_id: '777' } }, ctx({ owner: 'me' }, { meOwnerId: null }))).toBe(false);
    expect(leavesQueue({ props: { hubspot_owner_id: '777' } }, ctx({ owner: 'all' }))).toBe(false);
  });
});
