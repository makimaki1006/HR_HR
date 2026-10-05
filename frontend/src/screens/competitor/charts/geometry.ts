// グラフの寸法規則。旧 HTML (competitor_report.rs の chart() と
// section_06_demographics.rs の build_navy_pyramid_svg) の算術をそのまま移した。
// 描画 (SVG 要素) とは分けて、値から数値で検証できるようにしてある。
import { fixed } from '../format';
import type { PopulationBand } from '../../../generated/PopulationBand';

export interface BarDatum {
  label: string;
  count: number;
}

export interface BarOptions {
  /** 幅 900 (ヒストグラム) か 440 (キーワード)。 */
  wide: boolean;
  /** キーワード用: 高さ 450、ラベルは 60 度回転で常に全部出す。 */
  words: boolean;
}

export interface BarLayout {
  width: number;
  height: number;
  plot: number;
  max: number;
  gap: number;
  labelRotate: number;
  labelAnchor: 'start' | 'middle';
  labelFont: number;
  grid: { y: number; labelY: number; label: string }[];
  bars: {
    x: number;
    y: number;
    width: number;
    height: number;
    title: string;
    showLabel: boolean;
    label: string;
    labelX: number;
    labelY: number;
  }[];
}

export function barChartLayout(data: readonly BarDatum[], opt: BarOptions): BarLayout {
  const width = opt.wide ? 900 : 440;
  const height = opt.words ? 450 : 200;
  const bottom = opt.words ? 135 : 35;
  const plot = height - bottom - 10;
  const max = Math.max(1, ...data.map((d) => d.count));
  const gap = (width - 45) / data.length;
  const step = Math.max(Math.floor(data.length / 25), 1);

  const grid = [0, 1, 2, 3, 4].map((i) => {
    const y = 10 + plot * (1 - i / 4);
    return { y, labelY: y + 4, label: fixed((max * i) / 4, 0) };
  });

  const bars = data.map((d, i) => {
    const x = 40 + i * gap;
    const barHeight = (plot * d.count) / max;
    return {
      x,
      y: 10 + plot - barHeight,
      width: gap * 0.72,
      height: barHeight,
      title: `${d.label}: ${String(d.count)}件`,
      // キーワードは常に、ヒストグラムは 35 本未満なら全部、それ以上は間引く。
      showLabel: opt.words || data.length < 35 || i % step === 0,
      label: d.label,
      labelX: x + gap / 2,
      labelY: plot + 26,
    };
  });

  return {
    width,
    height,
    plot,
    max,
    gap,
    labelRotate: opt.words ? 60 : 0,
    labelAnchor: opt.words ? 'start' : 'middle',
    labelFont: opt.words ? 10 : 11,
    grid,
    bars,
  };
}

export interface PyramidLayout {
  width: number;
  height: number;
  center: number;
  barMax: number;
  max: number;
  rows: {
    label: string;
    y: number;
    male: number;
    female: number;
    maleX: number;
    maleWidth: number;
    femaleX: number;
    femaleWidth: number;
  }[];
}

const PYRAMID_WIDTH = 720;
const PYRAMID_ROW_H = 18;
const PYRAMID_LABEL_COL = 56;
const PYRAMID_CENTER_GAP = 8;

export function pyramidLayout(bands: readonly PopulationBand[]): PyramidLayout {
  const barMax = (PYRAMID_WIDTH - PYRAMID_LABEL_COL) / 2 - PYRAMID_CENTER_GAP;
  const center = PYRAMID_LABEL_COL + barMax + PYRAMID_CENTER_GAP;
  const max = Math.max(1, ...bands.flatMap((b) => [b.male, b.female]));
  // 上が高齢になるよう、年齢の若い順の配列を逆順に並べる。
  const rows = [...bands].reverse().map((b, i) => {
    const maleWidth = Math.max((b.male / max) * barMax, 0.5);
    const femaleWidth = Math.max((b.female / max) * barMax, 0.5);
    return {
      label: b.age_group,
      y: 36 + i * PYRAMID_ROW_H,
      male: b.male,
      female: b.female,
      maleX: center - (b.male / max) * barMax,
      maleWidth,
      femaleX: center,
      femaleWidth,
    };
  });
  return {
    width: PYRAMID_WIDTH,
    height: 40 + bands.length * PYRAMID_ROW_H + 24,
    center,
    barMax,
    max,
    rows,
  };
}
