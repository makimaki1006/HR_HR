import type { IndeedSection } from '../../../generated/IndeedSection';
import { fmtNumber } from '../format';

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
          <p className="cmp-note">
            出典: {data.source} / 集計日: {data.built_at}。{data.caveat}{' '}
            求人を見た人数は応募数ではありません。欠測は「—」で表示します。
          </p>
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
