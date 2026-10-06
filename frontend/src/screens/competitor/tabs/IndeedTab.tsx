import type { IndeedSection } from '../../../generated/IndeedSection';
import type { IndeedRow } from '../../../generated/IndeedRow';
import { TrendChart } from '../charts/TrendChart';
import { fmtNumber } from '../format';

const SERIES: { label: string; unit: string; color: string; ratio: boolean; pick: (r: IndeedRow) => number | null }[] = [
  { label: '求人数の推移', unit: '件', color: '#007d79', ratio: false, pick: (r) => r.job },
  { label: '求人を見た人数の推移', unit: '人', color: '#4472c4', ratio: false, pick: (r) => r.ctk },
  { label: '募集企業数の推移', unit: '社', color: '#8567a5', ratio: false, pick: (r) => r.emp },
  { label: '1求人あたりに見た人数', unit: '人/求人', color: '#b37d20', ratio: true, pick: (r) => r.spp },
];

export function IndeedTab({ data }: { data: IndeedSection }) {
  return (
    <section className="cmp-page" id="competitor-indeed">
      <header className="cmp-page-head">
        <span className="cmp-eyebrow">採用市場</span>
        <h2>Indeed採用レポート</h2>
        <p className="cmp-sub">選択した職種・都道府県の月別データ</p>
      </header>
      {data.status === 'unavailable' ? (
        <p className="cmp-note">{data.message}</p>
      ) : (
        <>
          <p>
            {data.title} / {data.region}
          </p>
          <p className="cmp-note">出典：Indeed採用市場レポート｜全給与形態。閲覧人数は応募数ではありません。</p>
          <div className="cmp-trend-grid">
            {SERIES.map((x) => (
              <TrendChart
                key={x.label}
                label={x.label}
                unit={x.unit}
                color={x.color}
                ratio={x.ratio}
                points={data.rows.map((r) => ({ month: r.month, value: x.pick(r) }))}
              />
            ))}
          </div>
          <table className="cmp-table">
            <thead>
              <tr>
                <th>月</th>
                <th>求人数</th>
                <th>求人を見た人数</th>
                <th>募集企業数</th>
                <th>1求人あたりに見た人数</th>
              </tr>
            </thead>
            <tbody>
              {data.rows.length === 0 && (
                <tr>
                  <td colSpan={5}>月別データがありません。0件を意味しません。</td>
                </tr>
              )}
              {data.rows.map((row) => (
                <tr key={row.month}>
                  <td>{row.month}</td>
                  <td className="cmp-num">{fmtNumber(row.job)}</td>
                  <td className="cmp-num">{fmtNumber(row.ctk)}</td>
                  <td className="cmp-num">{fmtNumber(row.emp)}</td>
                  <td className="cmp-num">{fmtNumber(row.spp)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </section>
  );
}
