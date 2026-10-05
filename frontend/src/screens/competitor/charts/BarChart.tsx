import { barChartLayout, type BarDatum } from './geometry';

interface Props {
  caption: string;
  data: readonly BarDatum[];
  wide?: boolean;
  words?: boolean;
}

/** 旧 chart() の SVG。viewBox は 900x200 (ヒストグラム) / 440x450 (キーワード)。 */
export function BarChart({ caption, data, wide = false, words = false }: Props) {
  return (
    <figure className={`cmp-chart${wide ? ' cmp-chart--wide' : ''}`}>
      <figcaption>{caption}</figcaption>
      {data.length === 0 ? (
        <p className="cmp-note">集計できるデータがありません</p>
      ) : (
        <Svg caption={caption} data={data} wide={wide} words={words} />
      )}
    </figure>
  );
}

function Svg({ caption, data, wide, words }: Required<Props>) {
  const l = barChartLayout(data, { wide, words });
  return (
    <svg viewBox={`0 0 ${String(l.width)} ${String(l.height)}`} role="img" aria-label={caption}>
      <title>{caption}</title>
      {l.grid.map((g, i) => (
        <g key={i}>
          <path d={`M 36 ${String(g.y)} H ${String(l.width)}`} className="cmp-gridline" />
          <text x={30} y={g.labelY} textAnchor="end" fontSize={11} className="cmp-axis">
            {g.label}
          </text>
        </g>
      ))}
      {l.bars.map((b, i) => (
        <g key={i}>
          <rect x={b.x} y={b.y} width={b.width} height={b.height} className="cmp-bar">
            <title>{b.title}</title>
          </rect>
          {b.showLabel && (
            <text
              transform={`translate(${String(b.labelX)} ${String(b.labelY)}) rotate(${String(l.labelRotate)})`}
              textAnchor={l.labelAnchor}
              fontSize={l.labelFont}
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
