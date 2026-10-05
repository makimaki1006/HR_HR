import type { PopulationBand } from '../../../generated/PopulationBand';
import { fmtInt } from '../format';
import { pyramidLayout } from './geometry';

/** 人口ピラミッド (旧 build_navy_pyramid_svg)。左 = 男性、右 = 女性。 */
export function Pyramid({ bands }: { bands: readonly PopulationBand[] }) {
  const l = pyramidLayout(bands);
  const axisY = l.height - 8;
  return (
    <svg
      viewBox={`0 0 ${String(l.width)} ${String(l.height)}`}
      width="100%"
      preserveAspectRatio="xMidYMid meet"
      role="img"
      aria-label="人口ピラミッド"
      className="cmp-pyramid"
    >
      <title>年齢階級別 人口ピラミッド</title>
      <text x={4} y={18} fontSize={10} fontWeight={700} className="cmp-axis">
        年齢
      </text>
      <text x={l.center - 8} y={18} fontSize={11} fontWeight={700} textAnchor="end" className="cmp-label">
        男性
      </text>
      <text x={l.center + 8} y={18} fontSize={11} fontWeight={700} className="cmp-label">
        女性
      </text>
      <line x1={l.center} y1={30} x2={l.center} y2={l.height - 24} className="cmp-gridline" strokeWidth={0.5} />
      {l.rows.map((r) => (
        <g key={r.label}>
          <rect x={r.maleX} y={r.y} width={r.maleWidth} height={14} className="cmp-male">
            <title>{`${r.label} 男性: ${fmtInt(r.male)}`}</title>
          </rect>
          <rect x={r.femaleX} y={r.y} width={r.femaleWidth} height={14} className="cmp-female">
            <title>{`${r.label} 女性: ${fmtInt(r.female)}`}</title>
          </rect>
          <text x={4} y={r.y + 10} fontSize={10} fontWeight={600} className="cmp-label">
            {r.label}
          </text>
        </g>
      ))}
      <text x={4} y={axisY} fontSize={9} className="cmp-axis">
        {fmtInt(l.max)} 名
      </text>
      <text x={l.width - 4} y={axisY} fontSize={9} textAnchor="end" className="cmp-axis">
        {fmtInt(l.max)} 名
      </text>
    </svg>
  );
}
