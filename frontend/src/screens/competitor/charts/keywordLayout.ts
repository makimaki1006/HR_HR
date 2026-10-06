// キーワードのグラフの寸法規則。旧 static/js/competitor-keywords.js の draw() の算術をそのまま移した。
// 描画 (SVG 要素) とは分けて、値から数値で検証できるようにしてある。
export interface KeywordBar {
  group: '全体' | '先頭';
  /** 比較は占有率 (%)、全体は件数。欠測は null (0 にしない)。 */
  value: number | null;
}

export interface KeywordLayoutInput {
  mode: 'all' | 'comparison';
  items: readonly { word: string; bars: readonly KeywordBar[] }[];
  /** 枠の実寸 (px)。 */
  w: number;
  h: number;
}

export interface KeywordLayout {
  w: number;
  h: number;
  left: number;
  top: number;
  ticks: { x: number; label: string }[];
  rows: {
    word: string;
    label: string;
    cy: number;
    bars: {
      group: '全体' | '先頭';
      value: number | null;
      y: number;
      width: number;
      height: number;
      color: string;
      /** 棒の右の数値 (比較は小数 1 桁の %、全体は件数)。 */
      text: string;
    }[];
  }[];
}

export function keywordLayout(input: KeywordLayoutInput): KeywordLayout {
  const { mode, items, w, h } = input;
  const comparison = mode === 'comparison';
  const left = Math.min(170, Math.max(86, w * 0.34));
  const right = 48;
  const plot = w - left - right;
  const top = comparison ? 42 : 20;
  const bottom = 30;
  const step = (h - top - bottom) / Math.max(1, items.length);
  const max = comparison ? 100 : Math.max(1, ...items.map((r) => r.bars[0]?.value ?? 0));
  const tickValues = comparison ? (w < 400 ? [0, 50, 100] : [0, 25, 50, 75, 100]) : [0, max / 2, max];
  const ticks = tickValues.map((t) => ({
    x: left + (plot * t) / max,
    label: comparison ? `${String(t)}%` : String(Math.round(t)),
  }));
  const labelLength = Math.max(4, Math.floor((left - 14) / 13));
  const rows = items.map((r, i) => {
    const y = top + i * step;
    const cy = y + step / 2;
    const chars = Array.from(r.word);
    const label = chars.length > labelLength ? `${chars.slice(0, labelLength - 1).join('')}…` : r.word;
    const offsets = comparison ? [cy - 11, cy + 1] : [cy - 10];
    const colors = comparison ? ['#006666', '#4472c4'] : ['#006666'];
    const height = comparison ? 9 : 20;
    return {
      word: r.word,
      label,
      cy,
      bars: r.bars.map((b, j) => ({
        group: b.group,
        value: b.value,
        y: offsets[j] ?? cy,
        width: b.value === null ? 0 : (plot * b.value) / max,
        height,
        color: colors[j] ?? '#006666',
        text: b.value === null ? '—' : comparison ? `${b.value.toFixed(1)}%` : `${String(b.value)}件`,
      })),
    };
  });
  return { w, h, left, top, ticks, rows };
}
