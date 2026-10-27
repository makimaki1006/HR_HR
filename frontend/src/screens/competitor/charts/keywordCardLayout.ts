// 検索語ごとのカード (1〜12 月の横軸に、年ごとに 1 本の線) の寸法規則。旧画面 (Rust の keyword_card) と同じ。
import type { GoogleKeyword } from '../../../generated/GoogleKeyword';
import { yearOverlay } from '../googleStats';

/** 年ごとの線の見た目 (新しい年から)。最新年ほど太い実線、古い年ほど細い破線。 */
export const YEAR_STYLES = [
  { color: '#006666', width: 2.8, dash: '' },
  { color: '#4472c4', width: 2.0, dash: '' },
  { color: '#b37d20', width: 1.8, dash: '6 3' },
  { color: '#8a949b', width: 1.6, dash: '2 3' },
] as const;

export interface KeywordCardLayout {
  width: number;
  height: number;
  grid: { y: number; value: number }[];
  /** 古い年から (最新年が前面)。rank は YEAR_STYLES の添字 (0 が最新)。 */
  years: {
    year: number;
    rank: number;
    segments: { x: number; y: number }[][];
    dots: { x: number; y: number; month: string; value: number }[];
  }[];
  peakX: number | null;
  months: { x: number; text: string }[];
  top: number;
  plotH: number;
  left: number;
  plotW: number;
}

const ok = (v: number | null | undefined): v is number => v !== null && v !== undefined && Number.isFinite(v) && v >= 0;

export function keywordCardLayout(k: GoogleKeyword, big: boolean, peakMonth: number | null): KeywordCardLayout | null {
  const o = yearOverlay(k);
  const shown = o.years
    .map((year, i) => ({ year, values: o.lines[i]?.values ?? [] }))
    .reverse()
    .slice(0, YEAR_STYLES.length);
  const observed = shown.flatMap((s) => s.values.filter(ok));
  if (observed.length === 0) return null;
  const [width, height, left] = big ? [900, 212, 85] : [450, 160, 55];
  const top = 12;
  const plotH = height - top - 32;
  const plotW = width - left - 14;
  const step = Math.max(Math.ceil(Math.max(...observed) / 2), 1);
  const ymax = step * 2;
  const yOf = (v: number): number => top + plotH * (1 - v / ymax);
  const xOf = (m: number): number => left + (plotW * m) / 11;
  const years = shown
    .map((s, rank) => {
      const segments: { x: number; y: number }[][] = [];
      let cur: { x: number; y: number }[] = [];
      const dots: KeywordCardLayout['years'][number]['dots'] = [];
      const flush = (): void => {
        if (cur.length > 1) segments.push(cur);
        cur = [];
      };
      s.values.forEach((v, i) => {
        if (ok(v)) {
          const p = { x: xOf(i), y: yOf(v) };
          cur.push(p);
          dots.push({ ...p, month: `${String(s.year)}-${String(i + 1).padStart(2, '0')}`, value: v });
        } else flush();
      });
      flush();
      return { year: s.year, rank, segments, dots };
    })
    .reverse();
  return {
    width,
    height,
    grid: [0, 1, 2].map((i) => ({ y: yOf(step * i), value: step * i })),
    years,
    peakX: peakMonth === null ? null : xOf(peakMonth - 1),
    months: Array.from({ length: 12 }, (_, m) => ({ x: xOf(m), text: String(m + 1) })),
    top,
    plotH,
    left,
    plotW,
  };
}
