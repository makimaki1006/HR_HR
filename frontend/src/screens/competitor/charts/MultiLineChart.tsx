import { fmtInt } from '../format';
import { multiLineLayout, type LineInput } from './multiLine';

const COLORS = [
  '#007d79',
  '#4472c4',
  '#8567a5',
  '#b37d20',
  '#c0504d',
  '#3b8f3b',
  '#d4608f',
  '#6b6b6b',
  '#17a2b8',
  '#9c6b3f',
];

interface Props {
  label: string;
  months: readonly string[];
  lines: readonly LineInput[];
  unit: string;
}

/** 複数の折れ線 (Google検索数の 2 系列・検索語の割合・閲覧数と検索数の指数)。凡例は図の中に入れる。 */
export function MultiLineChart({ label, months, lines: rawLines, unit }: Props) {
  // 名前の空の系列は描かない (凡例に空欄が出る)。
  const named = rawLines.filter((x) => x.label.trim() !== '');
  const layout = multiLineLayout(months, named);
  if (layout === null) return null;
  // 描く点が 1 つも無い系列は出さない。
  const visible = layout.lines.filter((x) => x.dots.length > 0);
  if (visible.length === 0) return null;
  const l = { ...layout, lines: visible };
  const lines = visible;
  const height = 280 + 22 * lines.length;
  return (
    <figure className="cmp-trend-card">
      <figcaption>{label}</figcaption>
      <div className="cmp-trend-plot">
        <svg viewBox={`0 0 900 ${String(height)}`} role="img" aria-label={label}>
          <title>{label}</title>
          {l.grid.map((g, i) => (
            <g key={i}>
              <path d={`M85 ${String(g.y)} H876`} className="cmp-gridline" />
              <text x={75} y={g.y + 5} fontSize={14} textAnchor="end" className="cmp-label">
                {fmtInt(g.value)}
              </text>
            </g>
          ))}
          {l.lines.map((line, k) => {
            const color = COLORS[k % COLORS.length];
            return (
              <g key={line.label} data-series={line.label}>
                {line.segments.map((seg, i) => (
                  <polyline
                    key={i}
                    className="trend-line"
                    points={seg.map((p) => `${p.x.toFixed(3)},${p.y.toFixed(3)}`).join(' ')}
                    fill="none"
                    stroke={color}
                    strokeWidth={3}
                  />
                ))}
                {line.dots.map((d, i) => (
                  <circle
                    key={i}
                    cx={d.x.toFixed(3)}
                    cy={d.y.toFixed(3)}
                    r={3.5}
                    fill="white"
                    stroke={color}
                    strokeWidth={2}
                    data-month={d.month}
                    data-value={d.value}
                    tabIndex={0}
                    aria-label={`${d.month}\u3000${line.label}: ${fmtInt(Math.round(d.value))} ${unit}`}
                  >
                    <title>{`${d.month}\u3000${line.label}: ${fmtInt(Math.round(d.value))} ${unit}`}</title>
                  </circle>
                ))}
              </g>
            );
          })}
          {l.monthLabels.map((m, i) => (
            <text key={i} x={m.x} y={268} textAnchor={m.end ? 'end' : 'middle'} fontSize={14} className="cmp-label">
              {m.text}
            </text>
          ))}
          {lines.map((line, k) => (
            <g key={line.label}>
              <rect x={85} y={286 + 22 * k} width={22} height={4} fill={COLORS[k % COLORS.length]} />
              <text x={115} y={292 + 22 * k} fontSize={14} className="cmp-label">
                {line.label}
              </text>
            </g>
          ))}
        </svg>
      </div>
      <p className="cmp-note">単位：{unit}</p>
    </figure>
  );
}
