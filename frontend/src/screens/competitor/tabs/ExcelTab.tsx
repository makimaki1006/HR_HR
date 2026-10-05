import type { CompetitorReport } from '../../../generated/CompetitorReport';
import type { KeywordRow } from '../../../generated/KeywordRow';
import type { SalaryRow } from '../../../generated/SalaryRow';
import { BarChart } from '../charts/BarChart';
import { fmtInt, fmtPct0, fmtSalary, MISSING } from '../format';

const WORD_ROWS = 10;
const CHART_WORDS = 25;

/** Excel 再現タブ。数値の加工 (換算・最頻値・フォールバック) は Rust 側で済んでいる。ここは整形と配置だけ。 */
export function ExcelTab({ report }: { report: CompetitorReport }) {
  const { meta, excel } = report;
  const d = excel.decimals;
  const unit = meta.unit;
  const topN = meta.top_n_effective;
  const wordChart = (rows: readonly KeywordRow[]): { label: string; count: number }[] =>
    rows.slice(0, CHART_WORDS).map((r) => ({ label: r.word, count: r.count }));

  return (
    <>
      <section className="cmp-excel" aria-label="競合調査ダッシュボード">
        <aside className="cmp-summary">
          <h2 className="cmp-summary-title">競合調査</h2>
          <table className="cmp-meta">
            <tbody>
              <tr>
                <th>調査名</th>
                <th>雇用形態</th>
              </tr>
              <tr>
                <td>{meta.title}</td>
                <td>{meta.employment_type ?? MISSING}</td>
              </tr>
              <tr>
                <th>該当都道府県</th>
                <th>主な市町村</th>
              </tr>
              <tr>
                <td>{meta.prefecture ?? MISSING}</td>
                <td>{meta.municipality ?? MISSING}</td>
              </tr>
              <tr>
                <th>集計対象</th>
                <th>該当件数</th>
              </tr>
              <tr>
                <td>CSV重複排除後</td>
                <td>{fmtInt(meta.total_count)}</td>
              </tr>
            </tbody>
          </table>

          <h2>給与関係（{unit}）</h2>
          <table>
            <thead>
              <tr>
                <th />
                <th colSpan={2}>総合</th>
                <th colSpan={2}>人気求人</th>
              </tr>
              <tr>
                <th />
                <th>下限</th>
                <th>上限</th>
                <th>下限</th>
                <th>上限</th>
              </tr>
            </thead>
            <tbody>
              {excel.salary_table.map((row: SalaryRow) => (
                <tr key={row.label}>
                  <th scope="row">{row.label}</th>
                  {row.values.map((v, i) => (
                    <td key={i}>{fmtSalary(v, d)}</td>
                  ))}
                </tr>
              ))}
              <tr>
                <th scope="row">集計件数</th>
                {excel.salary_counts.map((n, i) => (
                  <td key={i}>{n}</td>
                ))}
              </tr>
            </tbody>
          </table>

          <h2>差異（総合 − 人気求人）</h2>
          <table>
            <thead>
              <tr>
                <th />
                <th>下限</th>
                <th>上限</th>
              </tr>
            </thead>
            <tbody>
              {excel.salary_diff.map((row) => (
                <tr key={row.label}>
                  <th scope="row">{row.label}</th>
                  <td>{fmtSalary(row.values[0], d)}</td>
                  <td>{fmtSalary(row.values[1], d)}</td>
                </tr>
              ))}
            </tbody>
          </table>

          <KeywordTable title="求人票ワード調査（全体）" rows={excel.keyword_all} />
          <KeywordTable title={`求人票ワード調査（上位 ${String(topN)} 件）`} rows={excel.keyword_head} />
          <p className="cmp-note">
            給与比較はIndeed SPの主単位の求人を集計（SPデータがない場合、総合は給与分布と同じ対象）。人気求人はSPの「人気」「超人気」付き。最頻値は実額（同数は低い額）、未取得は
            —。少数の人気求人は参考値です。
          </p>
        </aside>

        <div className="cmp-charts">
          <BarChart
            caption={`上限ボリュームゾーン（${unit}）`}
            data={excel.histograms.upper}
            wide
          />
          <BarChart
            caption={`下限ボリュームゾーン（${unit}）`}
            data={excel.histograms.lower}
            wide
          />
          <BarChart caption="求人票キーワード調査（全体）" data={wordChart(excel.keyword_all)} words />
          <BarChart
            caption={`求人票キーワード調査（上位 ${String(topN)} 件）`}
            data={wordChart(excel.keyword_head)}
            words
          />
        </div>
      </section>
      <p className="cmp-caption">
        CSV重複排除後 {meta.total_count} 件 / 上位 {Math.min(topN, meta.total_count)}{' '}
        件は取り込み順の先頭。ワード表は上位10語、グラフは上位25語。給与分布は
        {meta.is_hourly ? '50円' : '1万円'}
        刻みで、月給モードは既存の月給換算値を使用。給与比較表とは対象が異なる場合があります。
      </p>
    </>
  );
}

function KeywordTable({ title, rows }: { title: string; rows: readonly KeywordRow[] }) {
  return (
    <>
      <h2>{title}</h2>
      <table className="cmp-words">
        <thead>
          <tr>
            <th>上位10件</th>
            <th>件数</th>
            <th>求人数</th>
            <th>占有率</th>
          </tr>
        </thead>
        <tbody>
          {rows.slice(0, WORD_ROWS).map((r, i) => (
            <tr key={i}>
              <td>{r.word}</td>
              <td>{r.count}</td>
              <td>{r.jobs}</td>
              <td>{fmtPct0(r.share_pct)}</td>
            </tr>
          ))}
          {rows.length === 0 && (
            <tr>
              <td colSpan={4}>キーワードデータがありません</td>
            </tr>
          )}
        </tbody>
      </table>
    </>
  );
}
