import { describe, expect, it } from 'vitest';
import { histogramLayout, pyramidLayout } from './geometry';
import { keywordLayout } from './keywordLayout';
import { trendLayout } from './trendLayout';

const bins = (counts: number[]): { label: string; count: number }[] =>
  counts.map((count, i) => ({ label: String(i), count }));

describe('histogramLayout (900x200、軸は件数 4 等分)', () => {
  // 旧 chart(): w=900 h=200 bottom=35 plot=141 max=ceil(peak/4)*4 gap=(w-64)/n
  const layout = histogramLayout(bins([2, 10, 5, 0, 7, 1, 3, 4, 6, 8, 9, 10]));

  it('寸法: 最大 10 件 → 目盛り間隔 3、軸の最大 12', () => {
    expect([layout.width, layout.height, layout.plot, layout.max, layout.peak]).toEqual([900, 200, 141, 12, 10]);
    expect(layout.gap).toBeCloseTo((900 - 64) / 12, 10);
  });

  it('棒の x・高さ・幅が JSON の値から決まる', () => {
    const [b0, b1, , b3] = layout.bars;
    const gap = (900 - 64) / 12;
    expect(b0?.x).toBeCloseTo(48 + gap * 0.08, 10);
    expect(b1?.x).toBeCloseTo(48 + gap + gap * 0.08, 10);
    expect(b0?.height).toBeCloseTo((141 * 2) / 12, 10);
    expect(b1?.height).toBeCloseTo((141 * 10) / 12, 10);
    expect(b3?.height).toBe(0); // 0 件は高さ 0 の棒として残る (欠測ではない)
    expect(b0?.width).toBeCloseTo(gap * 0.84, 10);
  });

  it('グリッド線 5 本と目盛り (0, 3, 6, 9, 12)', () => {
    expect(layout.grid.map((g) => g.label)).toEqual(['0', '3', '6', '9', '12']);
    expect(layout.grid[0]?.y).toBeCloseTo(165, 10);
    expect(layout.grid[4]?.y).toBeCloseTo(24, 10);
  });

  it('最大が 2 つ (10 件が 2 本) のときは棒の上の件数を出さず、色は両方 ピーク色', () => {
    expect(layout.peaks).toBe(2);
    expect(layout.bars.every((b) => b.topLabelY === null)).toBe(true);
    expect(layout.bars.filter((b) => b.fill === '#007d79')).toHaveLength(2);
  });

  it('最大が 1 つだけなら件数を棒の上 (y-7) に出す', () => {
    const one = histogramLayout(bins([1, 5, 2]));
    expect(one.peaks).toBe(1);
    expect(one.bars[1]?.topLabelY).toBeCloseTo((one.bars[1]?.y ?? 0) - 7, 10);
    expect(one.bars[0]?.topLabelY).toBeNull();
  });

  it('全部 0 件でも軸が壊れない (peak 1)', () => {
    const z = histogramLayout(bins([0, 0, 0]));
    expect(z.peak).toBe(1);
    expect(z.bars.every((b) => b.height === 0)).toBe(true);
  });

  it('1 件だけのとき、目盛りは 0..4 の整数', () => {
    expect(histogramLayout(bins([1])).grid.map((g) => g.label)).toEqual(['0', '1', '2', '3', '4']);
  });

  it('ラベルは 12 本以下なら全部、それ以上は ceil(n/12) 本おき (末尾は間隔が半分以上のときだけ)', () => {
    const shown = (n: number): number[] =>
      histogramLayout(bins(Array.from({ length: n }, () => 1)))
        .bars.map((b, i) => (b.showLabel ? i : -1))
        .filter((i) => i >= 0);
    expect(shown(12)).toHaveLength(12);
    // 31 本: stride 3、末尾 (30) は 30 % 3 = 0 なので通常の間引きで出る
    expect(shown(31)).toEqual([0, 3, 6, 9, 12, 15, 18, 21, 24, 27, 30]);
    // 29 本: stride 3、末尾 (28) は 28 % 3 = 1 >= floor(3/2)=1 なので追加
    expect(shown(29)).toEqual([0, 3, 6, 9, 12, 15, 18, 21, 24, 27, 28]);
  });
});

describe('keywordLayout: 旧 competitor-keywords.js の算術', () => {
  const items = [
    { word: '研修あり', bars: [{ group: '全体' as const, value: 40 }, { group: '先頭' as const, value: 75 }] },
    { word: '賞与あり', bars: [{ group: '全体' as const, value: null }, { group: '先頭' as const, value: 25 }] },
  ];

  it('比較: 軸は 0〜100% 共通。幅 400 以上は 5 目盛り', () => {
    const l = keywordLayout({ mode: 'comparison', items, w: 440, h: 715 });
    expect(l.ticks.map((t) => t.label)).toEqual(['0%', '25%', '50%', '75%', '100%']);
    const left = Math.min(170, Math.max(86, 440 * 0.34));
    const plot = 440 - left - 48;
    expect(l.left).toBeCloseTo(left, 10);
    expect(l.rows[0]?.bars[0]?.width).toBeCloseTo((plot * 40) / 100, 10);
    expect(l.rows[0]?.bars[1]?.width).toBeCloseTo((plot * 75) / 100, 10);
    expect(l.rows[0]?.bars[1]?.text).toBe('75.0%');
  });

  it('比較: 全体が欠測の語は棒を作らず — (0 にしない)', () => {
    const l = keywordLayout({ mode: 'comparison', items, w: 440, h: 715 });
    const missing = l.rows[1]?.bars[0];
    expect(missing?.value).toBeNull();
    expect(missing?.width).toBe(0);
    expect(missing?.text).toBe('—');
  });

  it('幅 400 未満は 3 目盛り', () => {
    expect(keywordLayout({ mode: 'comparison', items, w: 390, h: 715 }).ticks.map((t) => t.label)).toEqual([
      '0%',
      '50%',
      '100%',
    ]);
  });

  it('全体: 軸は最大件数まで。目盛りは 0・半分・最大', () => {
    const l = keywordLayout({
      mode: 'all',
      items: [
        { word: 'A', bars: [{ group: '全体', value: 30 }] },
        { word: 'B', bars: [{ group: '全体', value: 11 }] },
      ],
      w: 440,
      h: 715,
    });
    expect(l.ticks.map((t) => t.label)).toEqual(['0', '15', '30']);
    expect(l.rows[1]?.bars[0]?.text).toBe('11件');
    expect(l.top).toBe(20);
  });

  it('長い語は表示幅から決まる字数で … にする', () => {
    const l = keywordLayout({
      mode: 'all',
      items: [{ word: 'あいうえおかきくけこさしすせそ', bars: [{ group: '全体', value: 1 }] }],
      w: 440,
      h: 715,
    });
    expect(l.rows[0]?.label.endsWith('…')).toBe(true);
    expect(l.rows[0]?.word).toBe('あいうえおかきくけこさしすせそ');
  });
});

describe('trendLayout: 欠測は線を切り、観測した 0 は点にする', () => {
  it('観測値が 1 つも無ければ null', () => {
    expect(trendLayout([{ month: '2026-01', value: null }], false)).toBeNull();
  });

  it('0, 欠測, 4: 点は 2 つ、線は引かない (連続した 2 点が無い)', () => {
    const l = trendLayout(
      [
        { month: '2026-01', value: 0 },
        { month: '2026-02', value: null },
        { month: '2026-03', value: 4 },
      ],
      false,
    );
    expect(l?.dots).toHaveLength(2);
    expect(l?.dots[0]).toMatchObject({ month: '2026-01', value: 0 });
    expect(l?.segments).toHaveLength(0);
    expect(l?.dots[1]?.x).toBeCloseTo(876, 10);
    expect(l?.dots[1]?.y).toBeCloseTo(24, 10);
  });

  it('連続した 2 点以上は 1 本の線。欠測をはさむと別の線', () => {
    const l = trendLayout(
      [1, 2, null, 3, 4, 5].map((value, i) => ({ month: `2026-0${String(i + 1)}`, value })),
      false,
    );
    expect(l?.segments.map((s) => s.length)).toEqual([2, 3]);
  });

  it('比 (ratio) は 0 起点で 0.1 刻みの軸', () => {
    const l = trendLayout(
      [
        { month: '2026-01', value: 1 },
        { month: '2026-02', value: 2.5 },
      ],
      true,
    );
    expect(l?.step).toBeCloseTo(0.7, 10);
    expect(l?.ymax).toBeCloseTo(2.8, 10);
    expect(l?.dots[1]?.y).toBeCloseTo(24 + 212 * (1 - 2.5 / 2.8), 10);
  });

  it('負・非有限は欠測として扱い、0 にしない', () => {
    const l = trendLayout(
      [
        { month: 'a', value: -1 },
        { month: 'b', value: Number.NaN },
        { month: 'c', value: 2 },
      ],
      false,
    );
    expect(l?.dots).toHaveLength(1);
  });
});

describe('pyramidLayout (720 x 40+18n+24)', () => {
  const bands = [
    { age_group: '0～4歳', male: 100, female: 80 },
    { age_group: '5～9歳', male: 400, female: 200 },
  ];
  const l = pyramidLayout(bands);
  it('寸法', () => {
    expect(l.width).toBe(720);
    expect(l.height).toBe(40 + 2 * 18 + 24);
    expect(l.center).toBe(388);
    expect(l.barMax).toBe(324);
    expect(l.max).toBe(400);
  });
  it('先頭の行は最後の年齢帯 (上が高齢)', () => {
    expect(l.rows[0]?.label).toBe('5～9歳');
    expect(l.rows[1]?.label).toBe('0～4歳');
    expect(l.rows[0]?.y).toBe(36);
    expect(l.rows[1]?.y).toBe(54);
  });
  it('男性は中心から左、女性は右に伸びる', () => {
    const r0 = l.rows[0];
    expect(r0?.maleWidth).toBeCloseTo(324, 10);
    expect(r0?.maleX).toBeCloseTo(388 - 324, 10);
    expect(r0?.femaleWidth).toBeCloseTo(162, 10);
    expect(r0?.femaleX).toBe(388);
    expect(l.rows[1]?.maleWidth).toBeCloseTo(81, 10);
  });
  it('0 人でも幅 0.5 を残す', () => {
    const z = pyramidLayout([{ age_group: '0～4歳', male: 0, female: 0 }]);
    expect(z.rows[0]?.maleWidth).toBe(0.5);
    expect(z.max).toBe(1);
  });
});
