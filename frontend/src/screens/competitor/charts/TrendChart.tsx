import { fixed, fmtInt, MISSING } from '../format';
import { trendLayout, type TrendPoint } from './trendLayout';

interface Props {
  label: string;
  points: readonly TrendPoint[];
  unit: string;
  color: string;
  /** 1 求人あたりの人数のような比 (小数 2 桁、軸の刻みは 0.1 単位)。 */
  ratio?: boolean;
}

/**
 * 月ごとの折れ線 (旧 competitor_trends.rs の SVG)。欠測の月は線を切り、点も打たない (0 にしない)。
 * 観測した 0 は 0 の位置に点を打つ。観測値が 1 つも無ければ何も出さない。
 */
export function TrendChart({ label, points, unit, color, ratio = false }: Props) {
  const l = trendLayout(points, ratio);
  if (l === null) return null;
  const fmt = (v: number): string => (ratio ? fixed(v, 2) : fmtInt(Math.trunc(v)));
  const last = points[points.length - 1];
  const lastValue = last?.value ?? null;
  return (
    <figure className="cmp-trend-card">
      <figcaption>{label}</figcaption>
      {last && (
        <p className="cmp-trend-value">
          最終収録月 {last.month === '' ? MISSING : last.month}：
          {lastValue === null || !Number.isFinite(lastValue) || lastValue < 0 ? MISSING : fmt(lastValue)} {unit}
        </p>
      )}
      <div className="cmp-trend-plot">
        <svg viewBox="0 0 900 280" role="img" aria-label={label}>
          <title>{label}</title>
          {l.grid.map((g, i) => (
            <g key={i}>
              <path d={`M85 ${String(g.y)} H876`} className="cmp-gridline" />
              <text x={75} y={g.y + 5} fontSize={14} textAnchor="end" className="cmp-label">
                {fmt(g.value)}
              </text>
            </g>
          ))}
          {l.segments.map((seg, i) => (
            <polyline
              key={i}
              className="trend-line"
              points={seg.map((p) => `${p.x.toFixed(3)},${p.y.toFixed(3)}`).join(' ')}
              fill="none"
              stroke={color}
              strokeWidth={3}
            />
          ))}
          {l.dots.map((d, i) => (
            <circle
              key={i}
              cx={d.x.toFixed(3)}
              cy={d.y.toFixed(3)}
              r={4}
              fill="white"
              stroke={color}
              strokeWidth={2}
              data-month={d.month}
              data-value={d.value}
            >
              <title>{`${d.month}：${fmt(d.value)} ${unit}`}</title>
            </circle>
          ))}
          {l.monthLabels.map((m, i) => (
            <text key={i} x={m.x} y={268} textAnchor="middle" fontSize={14} className="cmp-label">
              {m.text}
            </text>
          ))}
        </svg>
      </div>
    </figure>
  );
}
