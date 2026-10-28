import { describe, expect, it } from 'vitest';
import type { GoogleKeyword } from '../../generated/GoogleKeyword';
import { keywordCardLayout } from './charts/keywordCardLayout';
import { allMonths, keywordLines, lowVolumeNote, monthYearPivot, summarize, yearOverlay } from './googleStats';

/** 開始月から n か月、値は f(i)。 */
function kw(keyword: string, start: [number, number], n: number, f: (i: number) => number | null): GoogleKeyword {
  const monthly = Array.from({ length: n }, (_, i) => {
    const t = start[0] * 12 + (start[1] - 1) + i;
    return {
      month: `${String(Math.floor(t / 12))}-${String((t % 12) + 1).padStart(2, '0')}`,
      search_volume: f(i),
    };
  });
  return { keyword, avg_monthly: 100, competition: 'LOW', monthly };
}

describe('yearOverlay / monthYearPivot', () => {
  it('暦年ごとに 1 本、1〜12 月に並べる。無い月は null', () => {
    // 2024-11 から 6 か月: 2024-11, 12, 2025-01..04
    const k = kw('a', [2024, 11], 6, (i) => (i + 1) * 10);
    const o = yearOverlay(k);
    expect(o.lines.map((l) => l.label)).toEqual(['2024', '2025']);
    expect(o.lines[0]?.values).toEqual([null, null, null, null, null, null, null, null, null, null, 10, 20]);
    expect(o.lines[1]?.values).toEqual([30, 40, 50, 60, null, null, null, null, null, null, null, null]);
    const p = monthYearPivot(k);
    expect(p.years).toEqual([2024, 2025]);
    expect(p.rows[0]).toEqual([null, 30]);
    expect(p.rows[10]).toEqual([10, null]);
  });
});

describe('summarize', () => {
  it('48 か月: 直近 12 か月合計と前年同期比', () => {
    // 2022-01 から 48 か月。2022〜2024 は 100、2025 は 150。
    const k = kw('a', [2022, 1], 48, (i) => (i >= 36 ? 150 : 100));
    const s = summarize(k);
    expect(s.last12Total).toBe(1800);
    // 2024 の 12 か月は 1200 → +50%
    expect(s.yoyPct).toBeCloseTo(50, 10);
  });

  it('13〜23 か月だと合計は出るが前年同期比は —(null)', () => {
    const k = kw('a', [2025, 1], 14, () => 10);
    const s = summarize(k);
    expect(s.last12Total).toBe(120);
    expect(s.yoyPct).toBeNull();
  });

  it('12 か月未満なら合計も null。欠測が窓に 1 つでもあれば null', () => {
    expect(summarize(kw('a', [2025, 1], 11, () => 10)).last12Total).toBeNull();
    const k = kw('a', [2023, 1], 36, (i) => (i === 30 ? null : 10));
    const s = summarize(k);
    expect(s.last12Total).toBeNull();
    expect(s.yoyPct).toBeNull();
  });

  it('前の 12 か月の合計が 0 なら前年同期比は null、観測した 0 は 0 として足す', () => {
    const k = kw('a', [2024, 1], 24, (i) => (i < 12 ? 0 : 5));
    const s = summarize(k);
    expect(s.last12Total).toBe(60);
    expect(s.yoyPct).toBeNull();
  });

  it('ピーク月 = 暦月ごとの平均が最大の月', () => {
    // 2024-01..2025-12。3 月: 2024=100, 2025=300 (平均 200)。7 月: 2024=250, 2025=100 (平均 175)。他は 10。
    const k = kw('a', [2024, 1], 24, (i) => {
      if (i === 2) return 100;
      if (i === 14) return 300;
      if (i === 6) return 250;
      if (i === 18) return 100;
      return 10;
    });
    expect(summarize(k).peakMonth).toBe(3);
  });

  it('2 年未満は最大の月。数値が無ければ null', () => {
    const k = kw('a', [2025, 1], 6, (i) => (i === 4 ? 99 : 5));
    expect(summarize(k).peakMonth).toBe(5);
    expect(summarize(kw('a', [2025, 1], 3, () => null)).peakMonth).toBeNull();
    expect(summarize({ keyword: 'x', avg_monthly: null, competition: '', monthly: [] }).peakMonth).toBeNull();
  });
});

describe('allMonths / keywordLines', () => {
  it('月の和集合を古い順に並べ、無い月は null', () => {
    const a = kw('a', [2025, 1], 3, () => 1);
    const b = kw('b', [2025, 2], 3, () => 2);
    const months = allMonths([a, b]);
    expect(months).toEqual(['2025-01', '2025-02', '2025-03', '2025-04']);
    const lines = keywordLines([a, b], months);
    expect(lines[0]?.values).toEqual([1, 1, 1, null]);
    expect(lines[1]?.values).toEqual([null, 2, 2, 2]);
  });
});

describe('月平均が少ない検索語のカード', () => {
  const mk = (v: number): GoogleKeyword => kw('x', [2025, 1], 24, () => v);
  it.each([0, 10])('全部 %i でも縦軸は 0 から有限の最大まで', (v) => {
    const l = keywordCardLayout(mk(v), false, null);
    expect(l).not.toBeNull();
    expect(l?.grid.map((g) => g.value)).toEqual([0, 1, 2].map((i) => Math.max(Math.ceil(v / 2), 1) * i));
    for (const y of l?.years ?? []) for (const p of y.dots) expect(Number.isFinite(p.y)).toBe(true);
  });
  it('添え書きは月平均と目安の断り', () => {
    expect(lowVolumeNote(mk(15))).toBe('月平均 15 回（丸め値が多く、動きは目安）');
  });
});
