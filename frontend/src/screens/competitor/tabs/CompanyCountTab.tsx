import type { PopularitySplit } from '../../../generated/PopularitySplit';
import { fmtInt, MISSING } from '../format';

export function CompanyCountTab({ data }: { data: PopularitySplit }) {
  const { rows, multi, single } = data.companies;
  return (
    <section className="cmp-page" id="competitor-company-count">
      <header className="cmp-page-head">
        <span className="cmp-eyebrow">求人の傾向</span>
        <h2>会社別の求人数</h2>
      </header>
      <p className="cmp-note">株式会社等を除いて同じ会社をまとめています。求人タイトルは表示しません。</p>
      <h3 className="cmp-block-title">求人数の多い会社</h3>
      <table className="cmp-table">
        <thead>
          <tr>
            <th>会社名</th>
            <th className="cmp-num">求人数</th>
            <th className="cmp-num">人気・超人気</th>
            <th className="cmp-num">うち超人気</th>
            <th className="cmp-num">最長の掲載</th>
          </tr>
        </thead>
        <tbody>
          {rows.length === 0 && (
            <tr>
              <td colSpan={5}>該当する会社がありません。</td>
            </tr>
          )}
          {rows.map((r, i) => (
            <tr key={`${r.name}-${String(i)}`}>
              <td>{r.name}</td>
              <td className="cmp-num">{fmtInt(r.n)}</td>
              <td className="cmp-num">{fmtInt(r.popular)}</td>
              <td className="cmp-num">{fmtInt(r.super_popular)}</td>
              <td className="cmp-num">
                {r.longest_is_30plus ? '30日以上' : r.longest_days === null ? MISSING : `${String(r.longest_days)}日`}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <h3 className="cmp-block-title">複数の求人を出す会社と、1件の会社</h3>
      <table className="cmp-table">
        <thead>
          <tr>
            <th />
            <th className="cmp-num">会社数</th>
            <th className="cmp-num">求人数</th>
            <th className="cmp-num">人気・超人気</th>
            <th className="cmp-num">割合</th>
          </tr>
        </thead>
        <tbody>
          {(
            [
              ['2件以上', multi],
              ['1件', single],
            ] as const
          ).map(([label, g]) => (
            <tr key={label}>
              <td>{label}</td>
              <td className="cmp-num">{fmtInt(g.companies)}</td>
              <td className="cmp-num">{fmtInt(g.n)}</td>
              <td className="cmp-num">{fmtInt(g.popular)}</td>
              <td className="cmp-num">{g.n === 0 ? MISSING : `${String(Math.round((g.popular / g.n) * 100))}%`}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}
