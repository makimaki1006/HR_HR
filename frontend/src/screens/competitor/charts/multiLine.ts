// 複数の折れ線を 1 枚に重ねるときの寸法規則 (trendLayout と同じ枠・目盛り)。
// 欠測 (null) は線を切る。観測した 0 は 0 のまま。縦軸は 0 始まりで全系列に共通。

export interface LineInput {
  label: string;
  values: readonly (number | null)[];
}

export interface MultiLineLayout {
  step: number;
  ymax: number;
  grid: { y: number; value: number }[];
  lines: {
    label: string;
    segments: { x: number; y: number }[][];
    dots: { x: number; y: number; month: string; value: number }[];
  }[];
  /** 最後の月を必ず表示し、そこから等間隔にさかのぼる。最後の月だけ右端に寄せる。 */
  monthLabels: { x: number; text: string; end: boolean }[];
}

const LEFT = 85;
const WIDTH = 791;
const TOP = 24;
const HEIGHT = 212;

/** 観測値が 1 つも無いときは null (グラフを出さない)。 */
export function multiLineLayout(months: readonly string[], lines: readonly LineInput[]): MultiLineLayout | null {
  const ok = (v: number | null | undefined): v is number => v !== null && v !== undefined && Number.isFinite(v) && v >= 0;
  const observed = lines.flatMap((l) => l.values.filter(ok));
  if (observed.length === 0) return null;
  const step = Math.max(Math.ceil(Math.max(...observed) / 4), 1);
  const ymax = step * 4;
  const grid = [0, 1, 2, 3, 4].map((i) => ({ y: TOP + HEIGHT * (1 - i / 4), value: step * i }));
  const xOf = (i: number): number => LEFT + (WIDTH * i) / Math.max(months.length - 1, 1);
  const out = lines.map((l) => {
    const segments: { x: number; y: number }[][] = [];
    let cur: { x: number; y: number }[] = [];
    const flush = (): void => {
      if (cur.length > 1) segments.push(cur);
      cur = [];
    };
    const dots: MultiLineLayout['lines'][number]['dots'] = [];
    months.forEach((m, i) => {
      const v = l.values[i];
      if (ok(v)) {
        const y = TOP + HEIGHT * (1 - v / ymax);
        cur.push({ x: xOf(i), y });
        dots.push({ x: xOf(i), y, month: m, value: v });
      } else {
        flush();
      }
    });
    flush();
    return { label: l.label, segments, dots };
  });
  const every = Math.max(Math.ceil(months.length / 8), 1);
  const lastIndex = months.length - 1;
  const monthLabels = months
    .map((m, i) => ({ x: xOf(i), text: m, i }))
    .filter((m) => (lastIndex - m.i) % every === 0)
    .map(({ x, text, i }) => ({ x, text, end: i === lastIndex }));
  return { step, ymax, grid, lines: out, monthLabels };
}
