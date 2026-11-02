import { useState } from 'react';
import { num } from './format';
import { Section } from './parts';
import type { ContactCell, TeamResponse } from './types';

const COLORS = ['#225d96', '#0f6b51', '#8b541b', '#5a3e8f', '#ab2f26', '#4a6572', '#b07d00', '#2d8a8a'];
const W = 720;
const H = 260;
const PAD = { l: 44, r: 16, t: 16, b: 36 };

/** 担当ごとの1件あたりの接触回数の推移 (旧画面の「接触の推移」と同じ値)。持ち案件が少ない点は打たない */
export function ContactTrend({ contact }: { contact: NonNullable<TeamResponse['contact']> }) {
  const [unit, setUnit] = useState<'month' | 'week'>('month');
  const u = contact[unit];
  const periods = (u?.periods ?? []).map((p, i) => ({ ...p, i })).filter((p) => p.calls_missing !== true);
  const minDeals = contact.meta?.min_deals ?? 3;
  const rows = (u?.rows ?? []).filter((r) => periods.some((p) => (r.cells[p.i]?.deals ?? 0) > 0));
  const point = (c: ContactCell | null | undefined): number | null =>
    c && (c.deals ?? 0) >= minDeals && c.small_n !== true && c.avg !== null && c.avg !== undefined ? c.avg : null;
  const team = u?.team ?? [];
  const all = [...rows.flatMap((r) => periods.map((p) => point(r.cells[p.i]))), ...periods.map((p) => point(team[p.i]))].filter((v): v is number => v !== null);
  const max = Math.max(1, ...all);
  const x = (k: number): number => PAD.l + (periods.length <= 1 ? 0 : (k * (W - PAD.l - PAD.r)) / (periods.length - 1));
  const y = (v: number): number => PAD.t + (1 - v / max) * (H - PAD.t - PAD.b);
  const line = (vals: (number | null)[]): string =>
    vals.reduce<string>((d, v, k) => (v === null ? d : `${d}${d === '' || vals[k - 1] === null ? 'M' : 'L'}${x(k).toFixed(1)},${y(v).toFixed(1)} `), '');
  return (
    <Section
      title="接触の推移（1件あたりの接触回数）"
      aside={
        <span className="cv2-seg" role="group" aria-label="期間の単位">
          <button type="button" aria-pressed={unit === 'month'} onClick={() => { setUnit('month'); }}>月</button>
          <button type="button" aria-pressed={unit === 'week'} onClick={() => { setUnit('week'); }}>週</button>
        </span>
      }
    >
      {periods.length === 0 ? (
        <p className="cv2-empty">通話の記録がある期間がありません。</p>
      ) : (
        <>
          <svg viewBox={`0 0 ${String(W)} ${String(H)}`} role="img" aria-label="担当ごとの1件あたりの接触回数の推移" className="cv2-chart">
            {[0, 0.5, 1].map((t) => (
              <g key={t}>
                <line x1={PAD.l} x2={W - PAD.r} y1={y(max * t)} y2={y(max * t)} stroke="#dae2eb" />
                <text x={PAD.l - 6} y={y(max * t) + 4} textAnchor="end" fontSize="11" fill="#607388">{num(max * t, 1)}</text>
              </g>
            ))}
            {periods.map((p, k) => (
              <text key={p.label} x={x(k)} y={H - 12} textAnchor="middle" fontSize="11" fill="#607388">{p.label}</text>
            ))}
            {rows.map((r, n) => (
              <path key={r.consultant} d={line(periods.map((p) => point(r.cells[p.i])))} fill="none" stroke={COLORS[n % COLORS.length]} strokeWidth="2" />
            ))}
            <path d={line(periods.map((p) => point(team[p.i])))} fill="none" stroke="#24384c" strokeWidth="2.5" strokeDasharray="5 4" />
          </svg>
          <ul className="cv2-legend">
            {rows.map((r, n) => (
              <li key={r.consultant}><i style={{ background: COLORS[n % COLORS.length] }} />{r.consultant}</li>
            ))}
            <li><i style={{ background: '#24384c' }} />チーム全体（点線）</li>
          </ul>
          <details className="cv2-more">
            <summary>数字を表で見る</summary>
            <div className="cv2-table-wrap">
              <table className="cv2-table">
                <thead>
                  <tr>
                    <th scope="col">担当者</th>
                    {periods.map((p) => <th key={p.label} scope="col" className="cv2-num">{p.label}{p.provisional === true ? '（途中）' : ''}</th>)}
                  </tr>
                </thead>
                <tbody>
                  {rows.map((r) => (
                    <tr key={r.consultant}>
                      <td>{r.consultant}</td>
                      {periods.map((p) => {
                        const c = r.cells[p.i];
                        return <td key={p.label} className="cv2-num">{c?.deals ? <>{num(c.avg ?? null, 2)}<small> {num(c.contacts ?? null)}/{num(c.deals)}件</small></> : '—'}</td>;
                      })}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </details>
          <p className="cv2-dim">持ち案件が {num(minDeals)} 件に満たない点は、たまたまで大きく振れるので図には打っていません（表には出ています）。担当者の評価ではありません。</p>
        </>
      )}
    </Section>
  );
}
