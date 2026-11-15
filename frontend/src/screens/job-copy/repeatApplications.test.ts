import { describe, expect, it } from 'vitest';
import { applicationEvents, REPEAT_APPLICATION_NOTE } from './listingEventsModel';
import { repeatApplicationRows } from './listingApplicationDates';

describe('同じ方の2回目以降の応募', () => {
  it('出来事には印を付けて出し、数える応募とは分ける', () => {
    const events = applicationEvents({
      applicationRows: [{ key: 'a', date: '2026-11-02', gender: '男性', age: '30', prefecture: '東京都', municipality: '江東区' }],
      repeatApplicationRows: [{ key: 'b', date: '2026-11-05' }, { key: 'c', date: null }],
    });
    expect(events.map(event => [event.date, event.summary, event.repeat ?? false])).toEqual([
      ['2026-11-02', '男性 ・ 30 ・ 東京都江東区', false],
      ['2026-11-05', REPEAT_APPLICATION_NOTE, true],
    ]);
    expect(REPEAT_APPLICATION_NOTE).toBe('同じ方の2回目の応募（数に入れません）');
    expect(events.filter(event => !event.repeat)).toHaveLength(1);
  });
  it('応答の印は鍵と日付だけを読み、ほかの値は捨てる', () => {
    expect(repeatApplicationRows([{ key: 'b', date: '2026-11-05', repeat: true, name: '架空' }, { key: '', date: '2026-11-06' }, { date: '2026-11-07' }, 'x', { key: 'd', date: '11/05' }])).toEqual([
      { key: 'b', date: '2026-11-05' },
      { key: 'd', date: null },
    ]);
    expect(repeatApplicationRows(undefined)).toEqual([]);
  });
});
