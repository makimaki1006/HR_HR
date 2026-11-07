import type { ReactNode } from 'react';
import { DASH, num, pct1 } from './format';
import type { BoxStats } from './types';

// ContactTrend と同じ SVG の描画。集計・分位点は API の値を使い、ここでは座標に変換するだけ。
export const CHART_MIN_N = 30;
export function DataTable({ title, heads, rows }: { title: string; heads: string[]; rows: ReactNode[][] }) {
  return <div className="cv2-table-wrap"><table className="cv2-table" aria-label={title}>
    <thead><tr>{heads.map((h) => <th scope="col" key={h}>{h}</th>)}</tr></thead>
    <tbody>{rows.map((r, i) => <tr key={i}>{r.map((v, j) => <td key={j}>{v}</td>)}</tr>)}</tbody>
  </table>{rows.length === 0 && <p className="cv2-empty">該当するデータはありません。</p>}</div>;
}
export function BoxPlot({ title, rows }: { title: string; rows: { label: string; stats: BoxStats | null | undefined; eligible?: boolean }[] }) {
  const shown = rows.filter((r): r is typeof r & { stats: BoxStats } => !!r.stats && r.stats.n >= CHART_MIN_N && r.eligible !== false);
  const lo = Math.min(0, ...shown.map((r) => r.stats.min));
  const hi = Math.max(lo + 1, ...shown.map((r) => r.stats.max));
  const x = (v: number): number => 150 + (v - lo) / (hi - lo) * 510;
  return <>
    {shown.length > 0 ? <svg className="cv2-chart" viewBox={`0 0 720 ${String(50 + shown.length * 42)}`} role="img" aria-label={title}>
      {shown.map(({ label, stats: s }, i) => <g key={label} transform={`translate(0 ${String(25 + i * 42)})`}>
        <title>{label}: 最小 {s.min}、下位4分の1 {s.q1}、中央値 {s.median}、上位4分の1 {s.q3}、最大 {s.max}、平均 {s.mean}、{s.n}件</title>
        <text x="4" y="4" fontSize="12">{label}</text>
        <line x1={x(s.min)} x2={x(s.max)} y1="0" y2="0" stroke="#225d96" />
        {[s.min, s.max].map((v, k) => <line key={k} x1={x(v)} x2={x(v)} y1="-7" y2="7" stroke="#225d96" />)}
        <rect x={x(s.q1)} y="-10" width={x(s.q3) - x(s.q1)} height="20" fill="#d8e6f3" stroke="#225d96" />
        <line x1={x(s.median)} x2={x(s.median)} y1="-10" y2="10" stroke="#225d96" strokeWidth="3" />
        <text x="710" y="4" textAnchor="end" fontSize="11">{s.n}件</text>
      </g>)}
      {[lo, hi].map((v) => <text key={v} x={x(v)} y={40 + shown.length * 42} textAnchor="middle" fontSize="11">{num(v, 1)}</text>)}
    </svg> : <p className="cv2-empty">図に載せられるデータがありません。</p>}
    <p className="cv2-dim">箱は下位4分の1から上位4分の1、太線は中央値、ひげは最小から最大。対象または値が入った契約が30件未満の区分は図に載せず、表に残しています。</p>
    <details className="cv2-more"><summary>数字を表で見る</summary>
      <DataTable title={title} heads={['区分', '件数', '最小', '下位4分の1', '中央値', '上位4分の1', '最大', '平均']} rows={rows.map(({ label, stats: s }) => [label, s ? num(s.n) : DASH, ...[s?.min, s?.q1, s?.median, s?.q3, s?.max, s?.mean].map((v) => v === undefined ? DASH : num(v, 2))])} />
    </details>
  </>;
}
export function Bars({ title, rows, percent = false }: { title: string; rows: { label: string; n: number | null }[]; percent?: boolean }) {
  const hi = Math.max(1, ...rows.map((r) => r.n ?? 0));
  return <svg className="cv2-chart" viewBox={`0 0 720 ${String(Math.max(45, rows.length * 32 + 10))}`} role="img" aria-label={title}>
    {rows.map((r, i) => <g key={r.label} transform={`translate(0 ${String(20 + i * 32)})`}>
      <title>{r.label}: {percent ? pct1(r.n) : num(r.n)}</title>
      <text x="4" y="4" fontSize="12">{r.label}</text>
      {r.n !== null && <rect x="200" y="-10" width={r.n / hi * 410} height="18" fill="#225d96" />}
      <text x="710" y="4" textAnchor="end" fontSize="12">{percent ? pct1(r.n) : num(r.n)}</text>
    </g>)}
  </svg>;
}
