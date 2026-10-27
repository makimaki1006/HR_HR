// 暦月の型 (年平均 = 100 の指数) の寸法規則。旧画面 (Rust の season_chart) と同じ枠・目盛り。
import type { IndeedSeason } from '../../../generated/IndeedSeason';

export interface SeasonLineInput {
  month: number;
  mean: number;
  /** 年ごとの最小〜最大。市場全体は null。 */
  range: [number, number] | null;
}

export interface SeasonLayout {
  grid: { y: number; value: number }[];
  baselineY: number;
  /** 今の月の帯の左端 x と中心 x。 */
  current: { x: number; cx: number } | null;
  band: string | null;
  line: string;
  /** 線の各点 (ホバー用の文字つき)。 */
  dots: { x: number; y: number; tip: string }[];
  /** Indeed の点。shape: 閲覧数 = diamond、求人数 = square。 */
  indeed: { x: number; y: number; tip: string; shape: 'diamond' | 'square'; label: string }[];
  months: { x: number; text: string }[];
  frame: SeasonFrame;
}

/** 画面幅が広いときの枠 (旧画面・PDF と同じ) と、スマホ向けの縦に高い枠。 */
export const SEASON_WIDE = { vw: 900, vh: 252, left: 85, width: 791, top: 24, height: 180, font: 14 } as const;
export const SEASON_NARROW = { vw: 450, vh: 330, left: 44, width: 392, top: 16, height: 250, font: 13 } as const;
export type SeasonFrame = typeof SEASON_WIDE | typeof SEASON_NARROW;

/** 全角空白 (ホバー文の区切り)。 */
const SP = '\u3000';
const r = (v: number): number => Math.round(v);

export function seasonLayout(
  lines: readonly SeasonLineInput[],
  currentMonth: number,
  indeed: IndeedSeason | null,
  compact = false,
): SeasonLayout | null {
  const frame: SeasonFrame = compact ? SEASON_NARROW : SEASON_WIDE;
  const { left: LEFT, width: WIDTH, top: TOP, height: HEIGHT } = frame;
  if (lines.length === 0) return null;
  let lo = 100;
  let hi = 100;
  for (const l of lines) {
    const [a, b] = l.range ?? [l.mean, l.mean];
    lo = Math.min(lo, a, l.mean);
    hi = Math.max(hi, b, l.mean);
  }
  for (const s of indeed?.series ?? []) {
    for (const p of s.points) {
      lo = Math.min(lo, p.index);
      hi = Math.max(hi, p.index);
    }
  }
  lo = Math.max(Math.floor((lo - 5) / 20) * 20, 0);
  hi = Math.ceil((hi + 5) / 20) * 20;
  const span = Math.max(hi - lo, 20);
  const yOf = (v: number): number => TOP + HEIGHT * (1 - (v - lo) / span);
  const xOf = (m: number): number => LEFT + (WIDTH * (m - 1)) / 11;
  const grid: SeasonLayout['grid'] = [];
  for (let g = lo; g <= hi + 0.001; g += 20) grid.push({ y: yOf(g), value: g });
  const fmt = (p: { x: number; y: number }): string => `${p.x.toFixed(1)},${p.y.toFixed(1)}`;
  let band: string | null = null;
  if (lines.every((l) => l.range !== null)) {
    const up = lines.map((l) => fmt({ x: xOf(l.month), y: yOf(l.range?.[1] ?? l.mean) }));
    const down = [...lines].reverse().map((l) => fmt({ x: xOf(l.month), y: yOf(l.range?.[0] ?? l.mean) }));
    band = [...up, ...down].join(' ');
  }
  const dots = lines.map((l) => ({
    x: xOf(l.month),
    y: yOf(l.mean),
    tip:
      l.range === null
        ? `${String(l.month)}月${SP}年平均比 ${String(r(l.mean))}`
        : `${String(l.month)}月${SP}年平均比 ${String(r(l.mean))}（最小 ${String(r(l.range[0]))}〜最大 ${String(r(l.range[1]))}）`,
  }));
  const ind: SeasonLayout['indeed'] = [];
  (indeed?.series ?? []).forEach((s, k) => {
    for (const p of s.points) {
      ind.push({
        x: xOf(p.month),
        y: yOf(p.index),
        tip: `${String(p.month)}月 ${String(p.year)}年${SP}${s.label} 年平均比 ${String(r(p.index))}`,
        shape: k === 0 ? 'diamond' : 'square',
        label: s.label,
      });
    }
  });
  return {
    grid,
    baselineY: yOf(100),
    current:
      currentMonth >= 1 && currentMonth <= 12 ? { x: xOf(currentMonth) - 18, cx: xOf(currentMonth) } : null,
    band,
    line: lines.map((l) => fmt({ x: xOf(l.month), y: yOf(l.mean) })).join(' '),
    dots,
    indeed: ind,
    months: Array.from({ length: 12 }, (_, i) => ({ x: xOf(i + 1), text: `${String(i + 1)}月` })),
    frame,
  };
}

