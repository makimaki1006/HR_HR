// 条件タグの使われ方のグラフの寸法。Rust の competitor_tag_usage.rs (旧 HTML・PDF) と同じ値と算術。
import type { TagBand } from '../../../generated/TagBand';
import type { TagUsage } from '../../../generated/TagUsage';
import type { TagUsageRow } from '../../../generated/TagUsageRow';

export const VIEW_W = 440;
const BAND_HEAD = 26;
const ROW_STEP = 22;
const BAND_GAP = 8;
const TOP = 6;
const AXIS_ROOM = 24;
export const CHART_ROWS_PER_BAND = 7;
export const GAP_CENTER = 285;
const GAP_HALF = 100;

/** 5 ポイント未満の端数は切り上げ、最小 10。 */
export function gapAxisMax(u: TagUsage): number {
  const m = [...u.popular_up, ...u.popular_down].reduce((a, r) => Math.max(a, Math.abs(r.gap_pt ?? 0)), 0);
  return Math.max(10, Math.ceil(m / 5) * 5);
}

export function signed(v: number): string {
  if (Math.abs(v) < 0.05) return '0.0';
  return `${v < 0 ? '-' : '+'}${Math.abs(v).toFixed(1)}`;
}

export function shortWord(word: string): string {
  const chars = Array.from(word);
  return chars.length > 10 ? `${chars.slice(0, 9).join('')}…` : word;
}

/** ホバーで出す説明 (語: 件数・割合 / 人気求人 / 差)。 */
export function rowTitle(r: TagUsageRow): string {
  let t = `${r.word}: ${String(r.n_tag)}件・${r.share_pct.toFixed(1)}%`;
  if (r.popular_share_pct !== null && r.gap_pt !== null) {
    t += ` / 人気求人 ${String(r.popular_tag_n)}件・${r.popular_share_pct.toFixed(1)}% / 差 ${signed(r.gap_pt)}`;
  }
  return t;
}

export interface BandLayout {
  band: TagBand;
  headY: number;
  /** 表示する語 (上位 CHART_ROWS_PER_BAND)。y は棒の上端。 */
  rows: { row: TagUsageRow; y: number; width: number }[];
  emptyY: number | null;
  moreY: number | null;
  more: number;
}

export function bandsLayout(u: TagUsage): { height: number; gridTop: number; gridBottom: number; bands: BandLayout[] } {
  let y = TOP;
  const gridTop = y;
  const bands = u.bands.map((band): BandLayout => {
    const headY = y;
    y += BAND_HEAD;
    let emptyY: number | null = null;
    if (band.rows.length === 0) {
      emptyY = y;
      y += ROW_STEP;
    }
    const rows = band.rows.slice(0, CHART_ROWS_PER_BAND).map((row) => {
      const out = { row, y, width: row.share_pct * 2.4 };
      y += ROW_STEP;
      return out;
    });
    let moreY: number | null = null;
    const more = Math.max(0, band.rows.length - CHART_ROWS_PER_BAND);
    if (more > 0) {
      moreY = y;
      y += 16;
    }
    y += BAND_GAP;
    return { band, headY, rows, emptyY, moreY, more };
  });
  return { height: y + AXIS_ROOM, gridTop, gridBottom: y, bands };
}

export interface GapSection {
  title: string;
  count: number;
  headY: number;
  emptyY: number | null;
  rows: { row: TagUsageRow; y: number; x: number; width: number; textX: number; anchor: 'start' | 'end'; positive: boolean }[];
}

export function gapLayout(u: TagUsage): { height: number; top: number; bottom: number; max: number; sections: GapSection[] } {
  const max = gapAxisMax(u);
  const per = GAP_HALF / max;
  let y = TOP;
  const top = y;
  const sections = (
    [
      ['人気求人に多い語', u.popular_up],
      ['人気求人に少ない語', u.popular_down],
    ] as const
  ).map(([title, list]): GapSection => {
    const headY = y;
    y += BAND_HEAD;
    let emptyY: number | null = null;
    if (list.length === 0) {
      emptyY = y;
      y += ROW_STEP;
    }
    const rows = list.map((row) => {
      const g = row.gap_pt ?? 0;
      const width = Math.abs(g) * per;
      const positive = g >= 0;
      const out = {
        row,
        y,
        x: positive ? GAP_CENTER : GAP_CENTER - width,
        width,
        textX: positive ? GAP_CENTER + width + 4 : GAP_CENTER - width - 4,
        anchor: positive ? ('start' as const) : ('end' as const),
        positive,
      };
      y += ROW_STEP;
      return out;
    });
    y += BAND_GAP;
    return { title, count: list.length, headY, emptyY, rows };
  });
  return { height: y + AXIS_ROOM, top, bottom: y, max, sections };
}
