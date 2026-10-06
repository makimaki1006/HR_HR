import type { ConsultationSection } from '../../../generated/ConsultationSection';
import { fixed, fmtSalary, MISSING } from '../format';

/** 符号つきの小数 (Rust の `{:+.N}`)。負の 0 は負として出す。 */
function signed(v: number, digits: number): string {
  const body = fixed(Math.abs(v), digits);
  return `${v < 0 || Object.is(v, -0) ? '-' : '+'}${body}`;
}

interface Props {
  data: ConsultationSection;
  /** 給与の単位 ("万円/月" か "円/時")。 */
  unit: string;
  /** 給与の小数桁 (月給 2、時給 0)。 */
  decimals: number;
}

/** 「採用のヒント」タブ。事実の比較だけを載せ、採用の見込みは言わない。 */
export function ConsultationTab({ data, unit, decimals }: Props) {
  return (
    <section className="cmp-page">
      <h2>採用のヒント</h2>
      <p className="cmp-note">競合データをもとに、給与・求人票・掲載後の反応を見直しましょう。</p>
      <div className="cmp-consultation-grid">
        <article className="cmp-consultation-card">
          <h3>1. 給与条件を見直す</h3>
          <p>{data.cohort}。人気求人：SPの「人気」「超人気」付き、選択単位の実額。</p>
          <table className="cmp-table">
            <thead>
              <tr>
                <th>中央値（{unit}）</th>
                <th>総合 / 有効件数</th>
                <th>人気 / 有効件数</th>
                <th>総合−人気</th>
              </tr>
            </thead>
            <tbody>
              {data.salary.map((row) => (
                <tr key={row.label}>
                  <th scope="row">{row.label}</th>
                  <td>
                    {fmtSalary(row.all_median, decimals)} / {row.all_n}件
                  </td>
                  <td>
                    {fmtSalary(row.popular_median, decimals)} / {row.popular_n}件
                  </td>
                  <td>{row.delta === null ? MISSING : signed(row.delta, decimals)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="cmp-note">
            給与分布の有効件数：下限 {data.distribution_min_n} 件・上限 {data.distribution_max_n} 件。
          </p>
          {data.small_sample && <p className="cmp-note">10件未満の比較は参考値です。</p>}
          <p>給与相場と自社の条件を比較し、勤務時間・手当も含めて見直しましょう。</p>
        </article>
        <article className="cmp-consultation-card">
          <h3>2. 求人票の訴求を見直す</h3>
          <p className="cmp-note">先頭の求人と全体を比較。先頭は収録順です。</p>
          <Gaps data={data} />
          <p>実際に提供できる待遇を、求人票で分かりやすく伝えましょう。</p>
        </article>
      </div>
      <h3>3. 掲載後の反応を確認する</h3>
      <table className="cmp-table">
        <tbody>
          {data.external.map((s) => (
            <tr key={s.label}>
              <th scope="row">{s.label}</th>
              <td>{s.fetched ? '取得済み' : '未取得'}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="cmp-note">外部データの対象地域・基準日は各タブに表示しています。</p>
      <p>職務内容に合う検索語を選び、掲載後の閲覧数・応募数・有効応募数を比較しましょう。</p>
    </section>
  );
}

function Gaps({ data }: { data: ConsultationSection }) {
  const g = data.gaps;
  if (g.status === 'insufficient') {
    return <p>先頭のタグ記録または比較母数が不足・不整合のため、差の判断を保留します。</p>;
  }
  if (g.status === 'no_lower_share') {
    return (
      <p>
        確認できるタグでは、全体より先頭の占有率が低い語はありません。訴求が十分であることの証明ではありません。
      </p>
    );
  }
  return (
    <table className="cmp-table">
      <thead>
        <tr>
          <th>語</th>
          <th>先頭 件/母数</th>
          <th>全体 件/母数</th>
          <th>差(pt)</th>
        </tr>
      </thead>
      <tbody>
        {g.rows.map((r) => (
          <tr key={r.word}>
            <td>{r.word}</td>
            <td>
              {r.head}/{g.head_n} ({fixed(r.head_share_pct, 1)}%)
            </td>
            <td>
              {r.all}/{g.all_n} ({fixed(r.all_share_pct, 1)}%)
            </td>
            <td>{signed(r.points, 1)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
