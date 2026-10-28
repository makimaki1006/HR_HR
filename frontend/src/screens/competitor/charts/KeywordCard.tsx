import type { GoogleKeyword } from '../../../generated/GoogleKeyword';
import { fixed, fmtInt, fmtNumber, MISSING } from '../format';
import { isLowVolume, lowVolumeNote, summarize, yoyWord } from '../googleStats';
import { keywordCardLayout, YEAR_STYLES } from './keywordCardLayout';
import { useNarrow } from './useNarrow';

const SP = '　';
const BADGE_COLOR = { 上昇: '#006666', 下降: '#b3261e', 横ばい: '#5b6770' } as const;

/**
 * 検索語 1 つ分のカード。1〜12 月の横軸に、年ごとに 1 本の線 (新しい方から 4 年) を重ねる。
 * 縦軸は 0 から自分の最大まで。月平均が少ない検索語も線は描くが、前年比・ピーク月は出さず、動きは目安だと添える。
 */
export function KeywordCard({ k, big }: { k: GoogleKeyword; big: boolean }) {
  const narrow = useNarrow();
  const low = isLowVolume(k);
  const s = summarize(k);
  const values = k.monthly.map((m) => m.search_volume).filter((v): v is number => v !== null);
  const max = values.length === 0 ? null : Math.max(...values);
  const peak = low ? null : s.peakMonth;
  const meta = [`平均 ${fmtNumber(k.avg_monthly)}`];
  meta.push(`最大 ${max === null ? MISSING : fmtInt(Math.round(max))}`);
  if (peak !== null) meta.push(`ピーク ${String(peak)}月`);
  const yoy = low ? null : s.yoyPct;
  const l = keywordCardLayout(k, big, peak, narrow);
  return (
    <figure className={`cmp-trend-card cmp-kwcard${big ? ' cmp-kwcard-main' : ''}`} data-keyword={k.keyword}>
      <figcaption className="cmp-kwcard-head">
        <span>{k.keyword}</span>
        <span className="cmp-kwcard-meta">
          {meta.join('・')}
          {yoy !== null && (
            <strong style={{ color: BADGE_COLOR[yoyWord(yoy)], marginLeft: 6 }}>
              前年比 {yoy >= 0 ? '+' : '-'}
              {fixed(Math.abs(yoy), 1)}%（{yoyWord(yoy)}）
            </strong>
          )}
        </span>
        {low && <span className="cmp-kwcard-meta">{lowVolumeNote(k)}</span>}
      </figcaption>
      {l !== null && (
        <div className="cmp-trend-plot cmp-fit">
          <svg viewBox={`0 0 ${String(l.width)} ${String(l.height)}`} role="img" aria-label={k.keyword}>
            <title>{k.keyword}</title>
            {l.grid.map((g) => (
              <g key={g.value}>
                <path d={`M${String(l.left)} ${g.y.toFixed(1)} H${String(l.left + l.plotW)}`} className="cmp-gridline" />
                <text x={l.left - 6} y={g.y + 5} fontSize={13} textAnchor="end" className="cmp-label">
                  {fmtInt(g.value)}
                </text>
              </g>
            ))}
            {l.peakX !== null && (
              <path d={`M${l.peakX.toFixed(1)} ${String(l.top)} V${String(l.top + l.plotH)}`} stroke="#8a949b" strokeDasharray="3 3" />
            )}
            {l.years.map((y) => {
              const st = YEAR_STYLES[y.rank] ?? YEAR_STYLES[0];
              return (
                <g key={y.year} data-year={y.year}>
                  {y.segments.map((seg, i) => (
                    <polyline
                      key={i}
                      points={seg.map((p) => `${p.x.toFixed(3)},${p.y.toFixed(3)}`).join(' ')}
                      fill="none"
                      stroke={st.color}
                      strokeWidth={st.width}
                      strokeDasharray={st.dash === '' ? undefined : st.dash}
                      strokeLinejoin="round"
                    />
                  ))}
                </g>
              );
            })}
            {l.months.map((m) => (
              <text key={m.text} x={m.x} y={l.monthY} textAnchor="middle" fontSize={13} className="cmp-label">
                {m.text}
              </text>
            ))}
            {l.years.flatMap((y) =>
              y.dots.map((d) => (
                <circle
                  key={d.month}
                  cx={d.x.toFixed(3)}
                  cy={d.y.toFixed(3)}
                  r={9}
                  fill="transparent"
                  tabIndex={0}
                  data-month={d.month}
                  data-value={d.value}
                  aria-label={`${d.month}${SP}${k.keyword}: ${fmtInt(Math.round(d.value))}回/月`}
                >
                  <title>{`${d.month}${SP}${k.keyword}: ${fmtInt(Math.round(d.value))}回/月`}</title>
                </circle>
              )),
            )}
          </svg>
          <ul className="cmp-season-legend">
            {l.years
              .slice()
              .reverse()
              .map((y) => {
                const st = YEAR_STYLES[y.rank] ?? YEAR_STYLES[0];
                return (
                  <li key={y.year}>
                    <svg width="26" height="8" aria-hidden="true">
                      <line x1="0" y1="4" x2="26" y2="4" stroke={st.color} strokeWidth={st.width} strokeDasharray={st.dash === '' ? undefined : st.dash} />
                    </svg>
                    {y.year}年
                  </li>
                );
              })}
          </ul>
        </div>
      )}
    </figure>
  );
}
