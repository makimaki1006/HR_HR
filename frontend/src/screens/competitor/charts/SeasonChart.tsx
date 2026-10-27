import type { IndeedSeason } from '../../../generated/IndeedSeason';
import { fmtInt } from '../format';
import { SEASON_AXIS, seasonLayout, type SeasonLineInput } from './seasonLayout';

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
  const l = seasonLayout(lines, currentMonth, indeed);
  if (l === null) return null;
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
      <div className="cmp-trend-plot">
        <svg viewBox="0 0 900 300" role="img" aria-label={label}>
          <title>{label}</title>
          {l.grid.map((g, i) => (
            <g key={i}>
              <path d={`M85 ${g.y.toFixed(1)} H876`} className="cmp-gridline" />
              <text x={75} y={g.y + 5} fontSize={14} textAnchor="end" className="cmp-label">
                {fmtInt(g.value)}
              </text>
            </g>
          ))}
          {l.current !== null && (
            <g data-testid="season-current">
              <rect x={l.current.x} y={SEASON_AXIS.top} width={36} height={SEASON_AXIS.height} fill="#f3e9c8" opacity={0.7} />
              <text x={l.current.cx} y={SEASON_AXIS.top + SEASON_AXIS.height + 36} fontSize={13} textAnchor="middle" fill="#8a6d1d">
                今月
              </text>
            </g>
          )}
          <path d={`M85 ${l.baselineY.toFixed(1)} H876`} stroke="#8a949b" strokeDasharray="5 4" />
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
            <text key={m.text} x={m.x} y={SEASON_AXIS.top + SEASON_AXIS.height + 22} textAnchor="middle" fontSize={14} className="cmp-label">
              {m.text}
            </text>
          ))}
          {legend.map((g, k) => {
            const lx = 85 + 220 * k;
            return (
              <g key={g.name}>
                {g.shape === 'line' && <rect x={lx} y={266} width={22} height={4} fill={g.color} />}
                {g.shape === 'diamond' && (
                  <path d={`M${String(lx + 11)} 262 l5.5 5.5 l-5.5 5.5 l-5.5 -5.5 z`} fill={g.color} />
                )}
                {g.shape === 'square' && <rect x={lx + 6} y={263} width={9} height={9} fill={g.color} />}
                <text x={lx + 30} y={272} fontSize={14} className="cmp-label">
                  {g.name}
                </text>
              </g>
            );
          })}
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
      <p className="cmp-note">縦軸は年平均を 100 とした指数です。</p>
    </figure>
  );
}
