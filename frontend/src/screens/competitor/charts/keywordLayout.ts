// キーワードのグラフの寸法規則。旧 static/js/competitor-keywords.js の draw() の算術をそのまま移した。
// 描画 (SVG 要素) とは分けて、値から数値で検証できるようにしてある。
export type KeywordGroup = '全体' | '先頭' | '人気・超人気';

/** 系列ごとの色 (旧 static/js/competitor-keywords.js と同じ)。 */
export const GROUP_COLORS: Record<KeywordGroup, string> = {
  全体: '#006666',
  先頭: '#4472c4',
  '人気・超人気': '#c0504d',
};

export interface KeywordBar {
  group: KeywordGroup;
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
  /** 数値の文字の大きさ (px)。 */
  labelSize: number;
  ticks: { x: number; label: string }[];
  rows: {
    word: string;
    label: string;
    cy: number;
    bars: {
      group: KeywordGroup;
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

/** 3 系列のとき、棒どうしの縦の間隔 (px)。棒の右の数値の文字 (11px) + 1px 以上にして数値を重ねない。 */
export const EXTRA_BAR_PITCH = 12;
/** 3 系列のとき、語ごとの縦幅の下限 (棒 3 本 + 語の間の余白 6px)。 */
export const EXTRA_MIN_STEP = 42;

export function keywordLayout(input: KeywordLayoutInput): KeywordLayout {
  const { mode, items, w } = input;
  const comparison = mode === 'comparison';
  const left = Math.min(170, Math.max(86, w * 0.34));
  const right = 48;
  const plot = w - left - right;
  // 人気・超人気の系列があるとき (棒が 3 本以上) は凡例を 2 行にして棒を細くする。
  const extra = comparison && Math.max(0, ...items.map((r) => r.bars.length)) > 2;
  const top = comparison ? (extra ? 58 : 42) : 20;
  const bottom = 30;
  // 3 系列は語ごとの縦幅を EXTRA_MIN_STEP 以上にし、足りない分は枠の高さを広げる。
  const h = extra ? Math.max(input.h, top + bottom + EXTRA_MIN_STEP * items.length) : input.h;
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
    const n = r.bars.length;
    const offsets = extra
      ? r.bars.map((_, j) => cy - (n * EXTRA_BAR_PITCH) / 2 + j * EXTRA_BAR_PITCH)
      : comparison
        ? [cy - 11, cy + 1]
        : [cy - 10];
    const height = extra ? 8 : comparison ? 9 : 20;
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
        color: GROUP_COLORS[b.group],
        text: b.value === null ? '—' : comparison ? `${b.value.toFixed(1)}%` : `${String(b.value)}件`,
      })),
    };
  });
  return { w, h, left, top, labelSize: comparison ? 11 : 13, ticks, rows };
}
