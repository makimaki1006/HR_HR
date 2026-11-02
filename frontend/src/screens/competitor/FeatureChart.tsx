import type { FeatureChart as FeatureChartData } from '../../generated/FeatureChart';
import type { FeatureRow } from '../../generated/FeatureRow';
import { fmtInt, MISSING } from './format';
import { isFew, legendLabels, orderRows, pctText, ratePct, readingSentence } from './featureModel';

const GREEN = '#006666';
const BLUE = '#4472c4';

function Lane({ popular, n, color, ariaLabel }: { popular: number; n: number; color: string; ariaLabel: string }) {
  const p = ratePct(popular, n);
  return (
    <div className="cmp-feat-srow" role="img" aria-label={ariaLabel}>
      <div className="cmp-feat-lane">
        {p !== null && <span className="cmp-feat-bar" style={{ width: `${String(p)}%`, background: color }} />}
      </div>
      <div className="cmp-feat-val">
        {pctText(popular, n)}
        <small>{`${fmtInt(n)}件中${fmtInt(popular)}件`}</small>
      </div>
    </div>
  );
}

function RawTable({ chart, rows }: { chart: FeatureChartData; rows: readonly FeatureRow[] }) {
  return (
    <details className="cmp-feat-raw">
      <summary>元の表・全数値を見る</summary>
      <div className="cmp-feat-scroll">
        <table className="cmp-feat-table">
          <thead>
            <tr>
              <th>項目</th>
              <th className="n">求人数</th>
              <th className="n">人気・超人気</th>
              <th className="n">人気・超人気の割合</th>
              <th className="n">{chart.band ? 'ほかの区分' : 'ない求人'}の割合</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.label}>
                <td>
                  {r.label}
                  {r.note !== null && <small className="cmp-feat-rownote">{r.note}</small>}
                  {r.uncertain && <small className="cmp-feat-rownote">分類に迷いが残る項目です</small>}
                </td>
                <td className="n">{fmtInt(r.n)}</td>
                <td className="n">{fmtInt(r.popular)}</td>
                <td className="n">{pctText(r.popular, r.n)}</td>
                <td className="n">{pctText(r.rest_popular, r.rest_n)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </details>
  );
}

export function FeatureChart({ chart }: { chart: FeatureChartData }) {
  const rows = orderRows(chart);
  const [first, second] = legendLabels(chart);
  const reading = readingSentence(chart, rows);
  return (
    <>
      <figure className="cmp-feat-chart" aria-label={`${chart.title}ごとの人気・超人気の割合`}>
        <figcaption>求人のうち、人気・超人気が占める割合</figcaption>
        {reading !== null && <p className="cmp-feat-eg">{reading}</p>}
        <ul className="cmp-feat-legend">
          <li>
            <i style={{ background: GREEN }} />
            {first}
          </li>
          <li>
            <i style={{ background: BLUE }} />
            {second}
          </li>
        </ul>
        <div className="cmp-feat-axis" aria-hidden="true">
          <span>0</span>
          <span>50</span>
          <span>100%</span>
        </div>
        {rows.length === 0 && <p className="cmp-feat-empty">{MISSING}</p>}
        {rows.map((r) => {
          const few = isFew(r.n);
          return (
            <div className="cmp-feat-row" key={r.label}>
              <div className="cmp-feat-label">
                {r.label}
                <small>{`求人数 ${fmtInt(r.n)}件${few ? '(少なめ)' : ''}`}</small>
              </div>
              <div className="cmp-feat-plot" style={few ? { opacity: 0.45 } : undefined}>
                <Lane popular={r.popular} n={r.n} color={GREEN} ariaLabel={`${r.label} ${first} ${pctText(r.popular, r.n)}`} />
                <Lane
                  popular={r.rest_popular}
                  n={r.rest_n}
                  color={BLUE}
                  ariaLabel={`${r.label} ${second} ${pctText(r.rest_popular, r.rest_n)}`}
                />
              </div>
            </div>
          );
        })}
      </figure>
      {chart.notes.map((t) => (
        <p className="cmp-feat-note2" key={t}>
          {t}
        </p>
      ))}
      <RawTable chart={chart} rows={rows} />
    </>
  );
}
