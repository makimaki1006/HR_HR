import type { HistogramSeries } from '../../../generated/HistogramSeries';
import { histogramLayout } from './geometry';

interface Props {
  caption: string;
  series: HistogramSeries;
}

/**
 * 給与分布の縦棒 (旧 chart())。空の給与区間も 0 件の棒として残し、軸を連続にする。
 * 最多の階級の一文 (summary) は Rust が作る。同数のピークは全部残る。
 */
export function SalaryHistogram({ caption, series }: Props) {
  return (
    <figure className="cmp-chart cmp-chart--wide cmp-salary-chart">
      <figcaption>{caption}</figcaption>
      {series.summary === null ? (
        <p className="cmp-note">集計できるデータがありません</p>
      ) : (
        <>
          <p className="cmp-salary-summary">
            <span className="cmp-salary-swatch" />
            {series.summary}
          </p>
          <Svg caption={caption} bins={series.bins} />
        </>
      )}
    </figure>
  );
}

function Svg({ caption, bins }: { caption: string; bins: HistogramSeries['bins'] }) {
  const l = histogramLayout(bins);
  return (
    <svg viewBox={`0 0 ${String(l.width)} ${String(l.height)}`} role="img" aria-label={caption}>
      <title>{caption}</title>
      {l.grid.map((g, i) => (
        <g key={i}>
          <path d={`M 48 ${String(g.y)} H ${String(l.width - 16)}`} className="cmp-gridline" />
          <text x={40} y={g.labelY} textAnchor="end" fontSize={14} className="cmp-axis">
            {g.label}
          </text>
        </g>
      ))}
      <text x={8} y={14} fontSize={13} className="cmp-axis">
        件数
      </text>
      {l.bars.map((b, i) => (
        <g key={i}>
          <rect x={b.x} y={b.y} width={b.width} height={b.height} rx={2} fill={b.fill}>
            <title>{b.title}</title>
          </rect>
          {b.topLabelY !== null && (
            <text
              x={b.labelX}
              y={b.topLabelY}
              textAnchor="middle"
              fontSize={15}
              fontWeight={700}
              className="cmp-peak-label"
            >
              {bins[i]?.count}件
            </text>
          )}
          {b.showLabel && (
            <text
              transform={`translate(${String(b.labelX)} ${String(b.labelY)}) rotate(0)`}
              textAnchor="middle"
              fontSize={14}
              className="cmp-axis"
            >
              {b.label}
            </text>
          )}
        </g>
      ))}
    </svg>
  );
}
