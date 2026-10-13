import { describe, expect, it } from 'vitest';
import {
  FIXTURE_ALL, FIXTURE_TODAY, asOfLabel, clockJst, filterByTeam, formatAvg, formatDuration, formatRate, sortRows, stripItems,
} from './metricsModel';

describe('formatDuration', () => {
  it('M:SS と H:MM:SS', () => {
    expect(formatDuration(0)).toBe('0:00');
    expect(formatDuration(59_999)).toBe('1:00');
    expect(formatDuration(108_000)).toBe('1:48');
    expect(formatDuration(3_312_000)).toBe('55:12');
    expect(formatDuration(3_600_000)).toBe('1:00:00');
    expect(formatDuration(4_140_000)).toBe('1:09:00');
    expect(formatDuration(null)).toBe('-');
  });
});

describe('割合・平均・時刻', () => {
  it('値の整形', () => {
    expect(formatRate(0.2115)).toBe('21.2%');
    expect(formatRate(0)).toBe('0.0%');
    expect(formatRate(null)).toBe('-');
    expect(formatAvg(47.54)).toBe('47.5');
    expect(formatAvg(undefined)).toBe('-');
  });
  it('JST の時刻', () => {
    // 05:05Z = JST 14:05
    expect(clockJst('2026-10-13T05:05:00Z')).toBe('14:05');
    expect(asOfLabel('2026-10-13T05:05:00Z')).toBe('14時05分時点');
    expect(clockJst('2026-10-12T15:00:00Z')).toBe('0:00');
    expect(clockJst(null)).toBeNull();
  });
});

describe('stripItems', () => {
  it('各項目が「自分の値（チーム平均・順位/人数）」にそろう', () => {
    const items = stripItems(FIXTURE_TODAY);
    expect(items?.map(i => [i.key, i.label, i.value, i.compare, i.extra])).toEqual([
      ['outbound', '架電数', '52', 'チーム平均 47.5・3位/8人', '着信 3'],
      ['over_90s', '90秒以上', '11・21.2%', 'チーム平均 8.4・2位/8人', null],
      ['duration', '通話時間', '55:12・1件 2:18', 'チーム平均 44:00・2位/8人', null],
      ['appointments', 'アポ', '2', 'チーム平均 1.1・1位/8人', null],
    ]);
  });
  it('比較が全員に落ちたら「全体平均」', () => {
    const items = stripItems({ ...FIXTURE_TODAY, population: 'all' });
    expect(items?.[0]?.compare).toBe('全体平均 47.5・3位/8人');
    expect(items?.[0]?.compareShort).toBe('平均47.5・3位/8');
  });
  it('本人が引けなければ null', () => {
    expect(stripItems({ ...FIXTURE_TODAY, me: null })).toBeNull();
  });
});

describe('管理者の表', () => {
  const names = (rows: typeof FIXTURE_ALL.callers) => rows.map(r => r.owner_id);
  it('降順・昇順。値が無い行は常に最後', () => {
    expect(names(sortRows(FIXTURE_ALL.callers, 'outbound', 'desc'))).toEqual(['9002', '9001', '9003', '9004', '9005']);
    expect(names(sortRows(FIXTURE_ALL.callers, 'outbound', 'asc'))).toEqual(['9005', '9004', '9003', '9001', '9002']);
    // 架電 0 の 9005 は平均通話時間が null: どちらの向きでも最後
    expect(names(sortRows(FIXTURE_ALL.callers, 'avg_duration', 'desc')).at(-1)).toBe('9005');
    expect(names(sortRows(FIXTURE_ALL.callers, 'avg_duration', 'asc')).at(-1)).toBe('9005');
    expect(names(sortRows(FIXTURE_ALL.callers, 'appointments', 'desc'))[0]).toBe('9003');
  });
  it('チームで絞る', () => {
    expect(filterByTeam(FIXTURE_ALL.callers, '架空営業チーム').map(r => r.owner_id)).toEqual(['9004']);
    expect(filterByTeam(FIXTURE_ALL.callers, '')).toHaveLength(5);
  });
});
