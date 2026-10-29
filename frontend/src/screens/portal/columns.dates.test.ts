import { setPortalPreviewClock } from './clock';
import { NOW } from './now';
import { beforeEach, describe, expect, it } from 'vitest';
import { dateSearchText, dayNum, dominantDateStyle, formatDateTimeJa, formatDayJa, writeDate } from './columns';
import type { ApplicantRow } from './types';

beforeEach(() => { setPortalPreviewClock(NOW); });

const row = (n: number, v: string): ApplicantRow => ({ rowNumber: n, values: { 面接予定日: v }, changedSinceLastView: null });

describe('formatDayJa', () => {
  it('今年の日付は月/日（曜日）', () => {
    expect(formatDayJa('2026/10/23')).toEqual({ text: '10/23（金）', isToday: false });
    expect(formatDayJa('2026年10月20日')).toEqual({ text: '10/20（火）', isToday: true });
  });
  it('年の無い値は応募日の年で当てる', () => {
    expect(formatDayJa('10月19日', dayNum(2026, 10, 17))?.text).toBe('10/19（月）');
    expect(formatDayJa('1月5日', dayNum(2026, 12, 20))?.text).toBe('2027/1/5（火）');
  });
  it('今年でなければ年を付ける', () => {
    expect(formatDayJa('2025/12/1')?.text).toBe('2025/12/1（月）');
  });
  it('読めない値は null', () => {
    expect(formatDayJa('記載なし')).toBeNull();
    expect(formatDayJa('来週')).toBeNull();
  });
});

describe('formatDateTimeJa', () => {
  it('別の列の時刻をつなぐ', () => {
    expect(formatDateTimeJa('2026/10/23', '11:00')?.text).toBe('10/23（金）11:00');
    expect(formatDateTimeJa('10月21日', '9時30分')?.text).toBe('10/21（水）9:30');
  });
  it('同じマスの時刻を読む', () => {
    expect(formatDateTimeJa('2026/10/24 14:00', undefined)?.text).toBe('10/24（土）14:00');
  });
  it('時刻が読めなければ日付だけ', () => {
    expect(formatDateTimeJa('2026/10/23', '未定')?.text).toBe('10/23（金）');
  });
});

describe('dateSearchText', () => {
  it('原文と表示の両方を含む', () => {
    const t = dateSearchText('2026年10月23日');
    expect(t).toContain('2026年10月23日');
    expect(t).toContain('10/23');
    expect(t).toContain('2026/10/23');
  });
});

describe('dominantDateStyle', () => {
  it('一番多い書き方', () => {
    const rows = [row(3, '2026年10月1日'), row(4, '2026年10月2日'), row(5, '2026/10/3'), row(6, '10月4日'), row(7, '記載なし')];
    expect(dominantDateStyle(rows, '面接予定日')).toBe('kanji');
  });
  it('無ければ slash', () => {
    expect(dominantDateStyle([row(3, '記載なし')], '面接予定日')).toBe('slash');
    expect(dominantDateStyle([row(3, '10月4日'), row(4, '2026/10/5')], '面接予定日')).toBe('slash');
  });
});

describe('writeDate', () => {
  it('列の書き方で書く', () => {
    expect(writeDate('2026-10-05', 'slash')).toBe('2026/10/05');
    expect(writeDate('2026-10-05', 'kanji')).toBe('2026年10月5日');
    expect(writeDate('2026-10-05', 'md')).toBe('10月5日');
    expect(writeDate('2027-01-05', 'md')).toBe('2027年1月5日');
    expect(writeDate('', 'slash')).toBe('');
  });
});
