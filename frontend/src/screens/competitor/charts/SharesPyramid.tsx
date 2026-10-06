import type { PopulationShares } from '../../../generated/PopulationShares';
import { fixed, fmtInt } from '../format';

/**
 * 年齢別・男女別の構成比のピラミッド (旧 competitor_population.rs)。
 * 分母は地域の総人口。左が男性、右が女性で、軸は両側とも 0 から axis_pct まで。
 */
export function SharesPyramid({ shares }: { shares: PopulationShares }) {
  const axis = shares.axis_pct;
  const height = 90 + shares.bands.length * 38;
  const rows = [...shares.bands].reverse();
  return (
    <svg
      viewBox={`0 0 1000 ${String(height)}`}
      role="img"
      aria-label="人口ピラミッド。左：男性、右：女性。分母は地域の総人口"
    >
      <title>年齢別・男女別の人口構成比</title>
      <text x={250} y={24} textAnchor="middle" fontSize={17} fill="#4472c4">
        男性
      </text>
      <text x={750} y={24} textAnchor="middle" fontSize={17} fill="#007d79">
        女性
      </text>
      {[0, 1, 2, 3, 4, 5].flatMap((i) => {
        const share = (axis * i) / 5;
        return [-1, 1].map((side) => {
          const x = (side < 0 ? 450 : 550) + ((side * share) / axis) * 400;
          return (
            <g key={`${String(i)}${String(side)}`}>
              <path d={`M${String(x)} 40 V${String(height - 38)}`} className="cmp-gridline" />
              <text x={x} y={height - 10} textAnchor="middle" fontSize={14} className="cmp-label">
                {fixed(share, 1)}%
              </text>
            </g>
          );
        });
      })}
      {rows.map((b, i) => {
        const y = 48 + i * 38;
        const male = (b.male_share_pct / axis) * 400;
        const female = (b.female_share_pct / axis) * 400;
        return (
          <g key={b.age_group}>
            <text x={500} y={y + 19} textAnchor="middle" fontSize={15} className="cmp-label">
              {b.age_group}
            </text>
            <rect
              x={450 - male}
              y={y}
              width={male}
              height={26}
              fill="#4472c4"
              data-age={b.age_group}
              data-sex="male_count"
              data-count={b.male}
              data-share={b.male_share_pct}
            >
              <title>{`${b.age_group}：${fmtInt(b.male)}人 / ${fixed(b.male_share_pct, 2)}%`}</title>
            </rect>
            <rect
              x={550}
              y={y}
              width={female}
              height={26}
              fill="#007d79"
              data-age={b.age_group}
              data-sex="female_count"
              data-count={b.female}
              data-share={b.female_share_pct}
            >
              <title>{`${b.age_group}：${fmtInt(b.female)}人 / ${fixed(b.female_share_pct, 2)}%`}</title>
            </rect>
          </g>
        );
      })}
    </svg>
  );
}
