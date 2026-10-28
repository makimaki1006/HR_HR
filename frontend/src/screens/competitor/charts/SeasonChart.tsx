import type { IndeedSeason } from '../../../generated/IndeedSeason';
import { fmtInt } from '../format';
import { seasonLayout, type SeasonLineInput } from './seasonLayout';
import { useNarrow } from './useNarrow';

const INDEED_COLORS = ['#c0504d', '#8567a5'];

interface Props {
  label: string;
  lineName: string;
  color: string;
  lines: readonly SeasonLineInput[];
  currentMonth: number;
  indeed: IndeedSeason | null;
}

/** 暦月 1〜12 の指数 (年平均 100) の折れ線・最小〜最大の帯・今の月の印。Indeed の点は線でつながない。 */
export function SeasonChart({ label, lineName, color, lines, currentMonth, indeed }: Props) {
  const narrow = useNarrow();
  const l = seasonLayout(lines, currentMonth, indeed, narrow);
  if (l === null) return null;
  const f = l.frame;
  const right = f.left + f.width;
  const legend: { name: string; color: string; shape: 'line' | 'diamond' | 'square' }[] = [
    { name: lineName, color, shape: 'line' },
    ...(indeed?.series ?? []).map((s, k) => ({
      name: `Indeed ${s.label}`,
      color: INDEED_COLORS[k % 2] ?? '#c0504d',
      shape: k === 0 ? ('diamond' as const) : ('square' as const),
    })),
  ];
  return (
    <figure className="cmp-trend-card">
      <figcaption>{label}</figcaption>
      <div className="cmp-trend-plot cmp-fit">
        <svg viewBox={`0 0 ${String(f.vw)} ${String(f.vh)}`} role="img" aria-label={label}>
          <title>{label}</title>
          {l.grid.map((g, i) => (
            <g key={i}>
              <path d={`M${String(f.left)} ${g.y.toFixed(1)} H${String(right)}`} className="cmp-gridline" />
              <text x={f.left - 10} y={g.y + 5} fontSize={f.font} textAnchor="end" className="cmp-label cmp-ylab">
                {fmtInt(g.value)}
              </text>
            </g>
          ))}
          {l.current !== null && (
            <g data-testid="season-current">
              <rect x={l.current.x} y={f.top} width={36} height={f.height} fill="#f3e9c8" opacity={0.7} />
              <text x={l.current.cx} y={f.top + f.height + (narrow ? 42 : 36)} fontSize={f.font - 1} textAnchor="middle" fill="#8a6d1d">
                今月
              </text>
            </g>
          )}
          <path d={`M${String(f.left)} ${l.baselineY.toFixed(1)} H${String(right)}`} stroke="#8a949b" strokeDasharray="5 4" />
          {l.band !== null && <polygon points={l.band} fill={color} opacity={0.16} stroke="none" />}
          <polyline points={l.line} fill="none" stroke={color} strokeWidth={3} />
          {l.indeed.map((p, i) => {
            const c = INDEED_COLORS[p.shape === 'diamond' ? 0 : 1] ?? '#c0504d';
            return p.shape === 'diamond' ? (
              <path
                key={i}
                d={`M${p.x.toFixed(1)} ${(p.y - 5.5).toFixed(1)} l5.5 5.5 l-5.5 5.5 l-5.5 -5.5 z`}
                fill={c}
                stroke="#fff"
                strokeWidth={1}
              />
            ) : (
              <rect key={i} x={p.x - 4.5} y={p.y - 4.5} width={9} height={9} fill={c} stroke="#fff" strokeWidth={1} />
            );
          })}
          {l.months.map((m) => (
            <text key={m.text} x={m.x} y={f.top + f.height + (narrow ? 20 : 22)} textAnchor="middle" fontSize={f.font} className="cmp-label">
              {m.text}
            </text>
          ))}
          {l.dots.map((d, i) => (
            <circle key={`g${String(i)}`} cx={d.x.toFixed(1)} cy={d.y.toFixed(1)} r={9} fill="transparent" tabIndex={0} aria-label={d.tip}>
              <title>{d.tip}</title>
            </circle>
          ))}
          {l.indeed.map((p, i) => (
            <circle key={`i${String(i)}`} cx={p.x.toFixed(1)} cy={p.y.toFixed(1)} r={9} fill="transparent" tabIndex={0} aria-label={p.tip}>
              <title>{p.tip}</title>
            </circle>
          ))}
        </svg>
      </div>
      {/* 凡例は図の外に置き、幅が狭いときは折り返す (図の中だと文字が重なる)。 */}
      <ul className="cmp-season-legend">
        {legend.map((g) => (
          <li key={g.name}>
            <svg width="22" height="14" aria-hidden="true">
              {g.shape === 'line' && <rect x={0} y={5} width={22} height={4} fill={g.color} />}
              {g.shape === 'diamond' && <path d="M11 1 l6 6 l-6 6 l-6 -6 z" fill={g.color} />}
              {g.shape === 'square' && <rect x={6} y={2} width={10} height={10} fill={g.color} />}
            </svg>
            {g.name}
          </li>
        ))}
      </ul>
      <p className="cmp-note">縦軸は、年平均を 100 とした値です。</p>
    </figure>
  );
}
