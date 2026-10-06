import type { CompetitorReport } from '../../../generated/CompetitorReport';
import type { KeywordRow } from '../../../generated/KeywordRow';
import type { SalaryRow } from '../../../generated/SalaryRow';
import type { KeywordComparison } from '../../../generated/KeywordComparison';
import { AllKeywordChart, ComparisonChart } from '../charts/KeywordChart';
import { SalaryHistogram } from '../charts/SalaryHistogram';
import { fixed, fmtInt, fmtPct0, fmtSalary, MISSING } from '../format';

const WORD_ROWS = 10;

/** 先頭率 − 全体率 (パーセントポイント)。0.05 未満は符号なしの 0.0。 */
function signedPoints(v: number): string {
  if (Math.abs(v) < 0.05) return '0.0';
  return `${v < 0 ? '-' : '+'}${fixed(Math.abs(v), 1)}`;
}

/** 給与・待遇タブ (旧「Excel再現」)。数値の加工 (換算・最頻値・フォールバック) は Rust 側で済んでいる。ここは整形と配置だけ。 */
export function ExcelTab({ report }: { report: CompetitorReport }) {
  const { meta, excel } = report;
  const d = excel.decimals;
  const unit = meta.unit;
  const cmp = excel.keyword_comparison;

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
          <ComparisonTable cmp={cmp} />
          <p className="cmp-note">人気求人：Indeedの「人気」「超人気」タグ付き。</p>
        </aside>

        <div className="cmp-charts">
          <SalaryHistogram
            caption={`上限ボリュームゾーン（${unit}・n=${String(excel.histograms.upper.n)}・${fixed(excel.histograms.upper.step, 0)}刻み）`}
            series={excel.histograms.upper}
          />
          <SalaryHistogram
            caption={`下限ボリュームゾーン（${unit}・n=${String(excel.histograms.lower.n)}・${fixed(excel.histograms.lower.step, 0)}刻み）`}
            series={excel.histograms.lower}
          />
          <AllKeywordChart rows={excel.keyword_all} />
          <ComparisonChart rows={cmp.rows} headN={cmp.head_n} allN={cmp.all_n} />
        </div>
      </section>
      <p className="cmp-caption">
        CSV重複排除後 {meta.total_count} 件 / 上位 {cmp.head_n} 件は収録順。占有率は語を含む求人数の割合。給与分布は
        {meta.is_hourly ? '時給の実額' : '月給換算'}。
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

/** 先頭 N 件と全体の占有率の比較表。全体に語が無いときは — (0 にしない)。 */
function ComparisonTable({ cmp }: { cmp: KeywordComparison }) {
  return (
    <>
      <h2>求人票ワード調査（先頭 {cmp.head_n} 件）</h2>
      <table className="cmp-words cmp-word-comparison">
        <thead>
          <tr>
            <th>上位10語</th>
            <th>件数</th>
            <th>先頭率</th>
            <th>全体率</th>
            <th>差(pt)</th>
          </tr>
        </thead>
        <tbody>
          {cmp.rows.slice(0, WORD_ROWS).map((r, i) => (
            <tr key={i}>
              <td>{r.word}</td>
              <td>{r.head_count}</td>
              <td>{fixed(r.head_share_pct, 1)}%</td>
              <td>{r.all_share_pct === null ? MISSING : `${fixed(r.all_share_pct, 1)}%`}</td>
              <td>{r.all_share_pct === null ? MISSING : signedPoints(r.head_share_pct - r.all_share_pct)}</td>
            </tr>
          ))}
          {cmp.rows.length === 0 && (
            <tr>
              <td colSpan={5}>
                {cmp.head_n === 0 ? '取り込み順の比較データがありません' : '先頭の求人にキーワードがありません'}
              </td>
            </tr>
          )}
        </tbody>
      </table>
      <p className="cmp-note cmp-word-basis">
        母数：先頭 {cmp.head_n} 件 / 全体 {cmp.all_n} 件。先頭は収録順、差は先頭率−全体率。
      </p>
    </>
  );
}
