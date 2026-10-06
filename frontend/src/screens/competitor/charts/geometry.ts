// グラフの寸法規則。旧 HTML (competitor_report.rs の chart() と
// section_06_demographics.rs の build_navy_pyramid_svg) の算術をそのまま移した。
// 描画 (SVG 要素) とは分けて、値から数値で検証できるようにしてある。
import { fixed } from '../format';

export interface BarDatum {
  label: string;
  count: number;
}

export interface HistogramLayout {
  width: number;
  height: number;
  plot: number;
  /** 軸の最大値 (目盛り間隔 x 4)。 */
  max: number;
  /** 最大の件数 (0 件のみのときは 1)。 */
  peak: number;
  /** 最大の件数に並んだ階級の数。1 のときだけ棒の上に件数を出す。 */
  peaks: number;
  gap: number;
  grid: { y: number; labelY: number; label: string }[];
  bars: {
    x: number;
    y: number;
    width: number;
    height: number;
    fill: string;
    title: string;
    label: string;
    showLabel: boolean;
    labelX: number;
    labelY: number;
    /** 最大の棒が 1 つだけのとき、棒の上に件数を出す位置。 */
    topLabelY: number | null;
  }[];
}

export const HIST_PEAK_FILL = '#007d79';
export const HIST_FILL = '#5d83b8';

/** 給与分布の縦棒。旧 chart() (competitor_report.rs) の算術。空の給与区間も 0 件の棒として残る。 */
export function histogramLayout(data: readonly BarDatum[]): HistogramLayout {
  const width = 900;
  const height = 200;
  const plot = height - 35 - 24;
  const peak = Math.max(1, ...data.map((d) => d.count));
  const tickStep = Math.ceil(peak / 4);
  const max = tickStep * 4;
  const gap = (width - 64) / data.length;
  const peaks = data.filter((d) => d.count === peak).length;
  const stride = Math.ceil(data.length / 12);

  const grid = [0, 1, 2, 3, 4].map((i) => {
    const y = 24 + plot * (1 - i / 4);
    return { y, labelY: y + 5, label: fixed((max * i) / 4, 0) };
  });

  const bars = data.map((d, i) => {
    const x = 48 + i * gap + gap * 0.08;
    const barHeight = (plot * d.count) / max;
    const y = 24 + plot - barHeight;
    return {
      x,
      y,
      width: gap * 0.84,
      height: barHeight,
      fill: d.count === peak ? HIST_PEAK_FILL : HIST_FILL,
      title: `${d.label}: ${String(d.count)}件`,
      label: d.label,
      showLabel:
        i % stride === 0 ||
        (i === data.length - 1 && i % stride >= Math.floor(stride / 2) && stride > 1),
      labelX: x + gap * 0.42,
      labelY: plot + 49,
      topLabelY: d.count === peak && peaks === 1 ? y - 7 : null,
    };
  });

  return { width, height, plot, max, peak, peaks, gap, grid, bars };
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

/** 年齢階級と男女の人数 (全部が数値のときだけ作る)。 */
export interface CompleteBand {
  age_group: string;
  male: number;
  female: number;
}

export function pyramidLayout(bands: readonly CompleteBand[]): PyramidLayout {
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
