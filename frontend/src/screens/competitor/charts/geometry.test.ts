import { describe, expect, it } from 'vitest';
import { barChartLayout, pyramidLayout } from './geometry';

const bins = (counts: number[]): { label: string; count: number }[] =>
  counts.map((count, i) => ({ label: String(i), count }));

describe('barChartLayout: ヒストグラム (wide, 900x200)', () => {
  // 旧 chart(): w=900 h=200 bottom=35 plot=155 max=max(count,1) gap=(w-45)/n
  const layout = barChartLayout(bins([2, 10, 5, 0, 7, 1, 3, 4, 6, 8, 9, 10]), {
    wide: true,
    words: false,
  });

  it('寸法', () => {
    expect([layout.width, layout.height, layout.plot, layout.max]).toEqual([900, 200, 155, 10]);
    expect(layout.gap).toBeCloseTo(71.25, 10);
  });

  it('棒の x・高さ・幅が JSON の値から決まる', () => {
    const [b0, b1, b2, b3] = layout.bars;
    expect(b0?.x).toBeCloseTo(40, 10);
    expect(b1?.x).toBeCloseTo(40 + 71.25, 10);
    expect(b0?.height).toBeCloseTo(155 * 0.2, 10); // 2/10
    expect(b1?.height).toBeCloseTo(155, 10); // 最大
    expect(b2?.height).toBeCloseTo(77.5, 10); // 5/10
    expect(b3?.height).toBe(0); // 0 件は高さ 0 (欠測ではない)
    expect(b1?.y).toBeCloseTo(10, 10); // 最大の棒の上端
    expect(b2?.y).toBeCloseTo(10 + 155 - 77.5, 10);
    expect(b0?.width).toBeCloseTo(71.25 * 0.72, 10);
  });

  it('グリッド線 5 本と目盛り (小数 0 桁、同点は偶数へ)', () => {
    expect(layout.grid).toHaveLength(5);
    expect(layout.grid.map((g) => g.label)).toEqual(['0', '2', '5', '8', '10']); // 2.5→2, 7.5→8
    expect(layout.grid[0]?.y).toBeCloseTo(165, 10);
    expect(layout.grid[4]?.y).toBeCloseTo(10, 10);
  });

  it('ラベルは 35 本未満なら全部出す', () => {
    expect(layout.bars.every((b) => b.showLabel)).toBe(true);
    expect(layout.bars[0]?.labelY).toBeCloseTo(155 + 26, 10);
    expect(layout.bars[1]?.labelX).toBeCloseTo(40 + 71.25 + 71.25 / 2, 10);
  });

  it('全部 0 件でも max は 1 で割り算が壊れない', () => {
    const z = barChartLayout(bins([0, 0, 0]), { wide: true, words: false });
    expect(z.max).toBe(1);
    expect(z.bars.every((b) => b.height === 0)).toBe(true);
  });
});

describe('barChartLayout: ラベルの間引き', () => {
  const shown = (n: number): number[] =>
    barChartLayout(
      bins(Array.from({ length: n }, () => 1)),
      { wide: true, words: false },
    )
      .bars.map((b, i) => (b.showLabel ? i : -1))
      .filter((i) => i >= 0);

  it('34 本は全部、35 本は step=max(floor(35/25),1)=1 で全部', () => {
    expect(shown(34)).toHaveLength(34);
    expect(shown(35)).toHaveLength(35);
  });
  it('50 本は step=2 で 25 個、75 本は step=3 で 25 個', () => {
    expect(shown(50)).toEqual(Array.from({ length: 25 }, (_, i) => i * 2));
    expect(shown(75)).toHaveLength(25);
    expect(shown(75)[1]).toBe(3);
  });
  it('60 本は step=2 で 30 個', () => {
    expect(shown(60)).toHaveLength(30);
  });
});

describe('barChartLayout: キーワード (440x450、常にラベル)', () => {
  const rows = [
    { label: '未経験', count: 40 },
    { label: '資格不問', count: 20 },
    { label: '寮あり', count: 5 },
  ];
  const l = barChartLayout(rows, { wide: false, words: true });
  it('寸法: h=450 bottom=135 plot=305 w=440', () => {
    expect([l.width, l.height, l.plot, l.max]).toEqual([440, 450, 305, 40]);
    expect(l.gap).toBeCloseTo((440 - 45) / 3, 10);
  });
  it('高さ', () => {
    expect(l.bars[0]?.height).toBeCloseTo(305, 10);
    expect(l.bars[1]?.height).toBeCloseTo(152.5, 10);
    expect(l.bars[2]?.height).toBeCloseTo(305 / 8, 10);
  });
  it('ラベルは回転 60 度・start・font 10、全部出る', () => {
    expect(l.labelRotate).toBe(60);
    expect(l.labelAnchor).toBe('start');
    expect(l.labelFont).toBe(10);
    expect(l.bars.every((b) => b.showLabel)).toBe(true);
  });
  it('ヒストグラムのラベルは回転 0・middle・font 11', () => {
    const h = barChartLayout(bins([1]), { wide: true, words: false });
    expect([h.labelRotate, h.labelAnchor, h.labelFont]).toEqual([0, 'middle', 11]);
  });
  it('title は「語: N件」', () => {
    expect(l.bars[0]?.title).toBe('未経験: 40件');
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
