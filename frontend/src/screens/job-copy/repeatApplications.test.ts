import { describe, expect, it } from 'vitest';
import { applicationEvents, countedApplications, REPEAT_APPLICATION_NOTE } from './listingEventsModel';
import { repeatApplicationRows } from './listingApplicationDates';

describe('同じ方の2回目以降の応募', () => {
  it('出来事には印を付けて出し、数える応募とは分ける', () => {
    const events = applicationEvents({
      applicationRows: [{ key: 'a', date: '2026-11-02', gender: '男性', age: '30', prefecture: '東京都', municipality: '江東区' }],
      repeatApplicationRows: [{ key: 'b', date: '2026-11-05', firstDate: '2026-11-02' }, { key: 'c', date: null, firstDate: null }],
    });
    expect(events.map(event => [event.date, event.summary, event.repeat ?? false, event.repeatOf ?? null])).toEqual([
      ['2026-11-02', '男性 ・ 30 ・ 東京都江東区', false, null],
      ['2026-11-05', REPEAT_APPLICATION_NOTE, true, '2026-11-02'],
    ]);
    expect(REPEAT_APPLICATION_NOTE).toBe('同じ方の2回目以降の応募（数に入れません）');
    // 「応募（N件）を出す」の N は除いた後の数
    expect(countedApplications(events)).toBe(1);
    expect(countedApplications([...events, { kind: 'content' }])).toBe(1);
  });
  it('応答の印は鍵と日付だけを読み、ほかの値は捨てる', () => {
    expect(repeatApplicationRows([{ key: 'b', date: '2026-11-05', first_date: '2026-11-02', repeat: true, name: '架空' }, { key: '', date: '2026-11-06' }, { date: '2026-11-07' }, 'x', { key: 'd', date: '11/05' }])).toEqual([
      { key: 'b', date: '2026-11-05', firstDate: '2026-11-02' },
      { key: 'd', date: null, firstDate: null },
    ]);
    expect(repeatApplicationRows(undefined)).toEqual([]);
  });
});
