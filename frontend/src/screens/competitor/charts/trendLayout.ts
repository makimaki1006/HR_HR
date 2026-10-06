// 月別の折れ線の寸法規則。旧 competitor_trends.rs の算術をそのまま移した。
// 欠測 (null) は線を切る。観測した 0 は 0 のまま点にする。

export interface TrendPoint {
  month: string;
  /** 欠測は null。 */
  value: number | null;
}

export interface TrendLayout {
  step: number;
  ymax: number;
  grid: { y: number; value: number }[];
  /** 連続して観測できた月ごとの線 (2 点以上のものだけ)。 */
  segments: { x: number; y: number }[][];
  dots: { x: number; y: number; month: string; value: number }[];
  monthLabels: { x: number; text: string }[];
}

const LEFT = 85;
const WIDTH = 791;
const TOP = 24;
const HEIGHT = 212;

/** 観測値が 1 つも無いときは null (グラフを出さない)。 */
export function trendLayout(points: readonly TrendPoint[], ratio: boolean): TrendLayout | null {
  const values = points.map((p) => (p.value !== null && Number.isFinite(p.value) && p.value >= 0 ? p.value : null));
  const observed = values.filter((v): v is number => v !== null);
  if (observed.length === 0) return null;
  const peak = Math.max(...observed);
  const step = ratio ? Math.max(Math.ceil((peak / 4) * 10), 1) / 10 : Math.max(Math.ceil(peak / 4), 1);
  const ymax = step * 4;
  const grid = [0, 1, 2, 3, 4].map((i) => ({ y: TOP + HEIGHT * (1 - i / 4), value: step * i }));
  const segments: { x: number; y: number }[][] = [];
  let current: { x: number; y: number }[] = [];
  const flush = (): void => {
    if (current.length > 1) segments.push(current);
    current = [];
  };
  const dots: TrendLayout['dots'] = [];
  const monthLabels: TrendLayout['monthLabels'] = [];
  const every = Math.max(Math.ceil(points.length / 8), 1);
  points.forEach((p, i) => {
    const x = LEFT + (WIDTH * i) / Math.max(points.length - 1, 1);
    const v = values[i] ?? null;
    const month = p.month === '' ? '—' : p.month;
    if (v !== null) {
      const y = TOP + HEIGHT * (1 - v / ymax);
      current.push({ x, y });
      dots.push({ x, y, month, value: v });
    } else {
      flush();
    }
    if (i % every === 0 || i === points.length - 1) monthLabels.push({ x, text: month });
  });
  flush();
  return { step, ymax, grid, segments, dots, monthLabels };
}
