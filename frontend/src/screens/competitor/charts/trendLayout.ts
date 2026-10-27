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
  /** 最後の月は end (右端に寄せる)。 */
  monthLabels: { x: number; text: string; end: boolean }[];
}

const LEFT = 85;
const WIDTH = 791;
const TOP = 24;
const HEIGHT = 212;

/** 横軸ラベルを出す月か。最後の月を必ず出し、そこから等間隔にさかのぼる (Rust の axis_label_shown と同じ)。 */
export function axisLabelShown(i: number, n: number): boolean {
  const every = Math.max(Math.ceil(n / 8), 1);
  return n > 0 && i >= 0 && i < n && (n - 1 - i) % every === 0;
}

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
    if (axisLabelShown(i, points.length)) monthLabels.push({ x, text: month, end: i === points.length - 1 });
  });
  flush();
  return { step, ymax, grid, segments, dots, monthLabels };
}
