import { describe, expect, it } from 'vitest';
import { addDays, changeWindow, unpublishedRanges, weekStraddlesChange } from './timelineModel';

describe('change windows on JST calendar days', () => {
  // Wednesday 9/16; every day before has 1 application, every day after has 2.
  const daily = Object.fromEntries(Array.from({ length: 56 }, (_, i) => [addDays('2026-08-19', i), i < 28 ? 1 : 2]));
  it('counts concrete 14-day totals across a Wednesday, including the change day after', () => {
    const result = changeWindow(daily, '2026-09-16', null, null, '2026-10-19');
    expect(result?.before).toMatchObject({ start: '2026-09-02', end: '2026-09-16', days: 14, count: 14 });
    expect(result?.after).toMatchObject({ start: '2026-09-16', end: '2026-09-30', days: 14, count: 28 });
    expect(changeWindow({ '2026-09-16': 5 }, '2026-09-16', null, null, '2026-10-19')?.after.count).toBe(5);
    expect(changeWindow({ '2026-09-16': 5 }, '2026-09-16', null, null, '2026-10-19')?.before.count).toBe(0);
  });
  it('stops before the next change and starts at the previous change', () => {
    const result = changeWindow(daily, '2026-09-16', '2026-09-10', '2026-09-25', '2026-10-19');
    expect(result?.before).toMatchObject({ start: '2026-09-10', days: 6, count: 6, perDay: 1 });
    expect(result?.after).toMatchObject({ end: '2026-09-25', days: 9, count: 18, perDay: 2 });
  });
  it('includes asOf and excludes future days', () => {
    expect(changeWindow(daily, '2026-09-16', null, null, '2026-09-20')?.after).toMatchObject({ days: 5, count: 10, end: '2026-09-21' });
    expect(changeWindow(daily, '2026-09-16', null, null, '2026-09-16')?.after.days).toBe(1);
    expect(changeWindow(daily, '2026-09-16', null, null, '2026-09-15')).toBeNull();
  });
  it.each([7, 28] as const)('switches to %i days', n => {
    const result = changeWindow(daily, '2026-09-16', null, null, '2026-10-19', n);
    expect(result?.before.count).toBe(n);
    expect(result?.after.count).toBe(2 * n);
    expect(result?.after.days).toBe(n);
  });
  it('hides unavailable daily data and preserves fetched zero counts', () => {
    expect(changeWindow(undefined, '2026-09-16', null, null, '2026-10-19')).toBeNull();
    expect(changeWindow({}, '2026-09-16', null, null, '2026-10-19')?.after.count).toBe(0);
  });
  it('counts unpublished days once per calendar day on both sides', () => {
    const gaps = [{ start: '2026-09-13', end: '2026-09-18' }, { start: '2026-09-14', end: '2026-09-17' }];
    const result = changeWindow(daily, '2026-09-16', null, null, '2026-10-19', 14, gaps);
    expect(result?.before.unpublishedDays).toBe(3);
    expect(result?.after.unpublishedDays).toBe(2);
    const period = { versionId: 'v1', label: '架空の本文', start: '2026-09-01', end: '2026-09-13', days: 12, ongoing: false, certainty: 'confirmed', basis: 'published' } as const;
    expect(unpublishedRanges([period, { ...period, start: '2026-09-18', end: null }], '2026-09-20')).toEqual([{ start: '2026-09-13', end: '2026-09-18' }]);
    expect(unpublishedRanges([{ ...period, basis: 'captured' }], '2026-09-20')).toEqual([]);
    expect(unpublishedRanges([period], '2026-09-15')).toEqual([{ start: '2026-09-13', end: '2026-09-16' }]);
  });
  it('marks weeks with days on both sides without splitting totals', () => {
    expect(weekStraddlesChange('2026-09-14', '2026-09-20', '2026-09-16')).toBe(true);
    expect(weekStraddlesChange('2026-09-14', '2026-09-20', '2026-09-14')).toBe(false);
    expect(weekStraddlesChange('2026-09-14', '2026-09-20', '2026-09-20')).toBe(true);
    expect(weekStraddlesChange('2026-09-14', '2026-09-20', '2026-09-21')).toBe(false);
  });
});
