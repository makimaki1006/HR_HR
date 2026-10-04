// React 版の日付処理 (md / wd / ago / daySpan) は実行環境の時刻帯に左右されない。
// 旧画面の JS はローカル時刻で読むため、テスト全体は Asia/Tokyo に固定している (vite.config.ts)。
// ここでは TZ を切り替えても React 側の表示が変わらないことを確かめる。
import { afterEach, describe, expect, it } from 'vitest';
import { ago, daySpan, md, wd } from './calc';

const ZONES = ['Asia/Tokyo', 'UTC', 'America/Los_Angeles', 'Pacific/Kiritimati'];
const saved = process.env.TZ;

afterEach(() => {
  process.env.TZ = saved;
});

describe('日付処理は時刻帯に依存しない', () => {
  it('テストの時刻帯は Asia/Tokyo に固定されている', () => {
    expect(process.env.TZ).toBe('Asia/Tokyo');
    expect(new Date('2026-09-03T00:00:00+09:00').getDate()).toBe(3);
  });

  for (const tz of ZONES) {
    it(`${tz}: md / wd / ago / daySpan が同じ結果`, () => {
      process.env.TZ = tz;
      expect(md('2026-09-03')).toBe('9/3');
      expect(wd('2026-09-03')).toBe('木');
      expect(md('2026-10-01')).toBe('10/1');
      expect(wd('2026-10-04')).toBe('日');
      expect(ago('2026-09-04', '2026-09-01')).toBe(3);
      expect(daySpan('2026-08-31', '2026-09-03')).toEqual([
        '2026-08-31',
        '2026-09-01',
        '2026-09-02',
        '2026-09-03',
      ]);
    });
  }
});
