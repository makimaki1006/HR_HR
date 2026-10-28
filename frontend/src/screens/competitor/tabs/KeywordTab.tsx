import type { CompetitorReport } from '../../../generated/CompetitorReport';
import type { TagUsage } from '../../../generated/TagUsage';
import { POPULARITY_ONLY_SP } from '../charts/KeywordChart';
import { TagBandsChart, TagGapChart } from '../charts/TagUsageCharts';
import { signed } from '../charts/tagUsageLayout';
import { fixed, MISSING } from '../format';

/** キーワードタブ。全体の割合の帯別の表 3 枚と、帯別・人気求人との差のグラフ。数え方は Rust 側で済んでいる。 */
export function KeywordTab({ report }: { report: CompetitorReport }) {
  const usage = report.excel.tag_usage;
  return (
    <section className="cmp-excel cmp-kw-tab" aria-label="キーワード">
      <aside className="cmp-summary">
        <h2 className="cmp-summary-title">キーワード</h2>
        <KeywordBandTables usage={usage} />
      </aside>
      <div className="cmp-charts">
        <TagBandsChart usage={usage} />
        <TagGapChart usage={usage} />
      </div>
    </section>
  );
}

const TABLE_ROWS_PER_BAND = 8;

/** 3 つの帯の表 (語・件数・全体の割合・人気求人での割合・差)。n を必ず添える。 */
function KeywordBandTables({ usage }: { usage: TagUsage }) {
  const hasPopular = usage.popular_n > 0;
  const cols = hasPopular ? 5 : 3;
  return (
    <>
      <h2>求人票キーワード調査（帯別）</h2>
      {usage.bands.map((b) => (
        <div key={b.key}>
          <h3 className="cmp-band-title">{`${b.label} ${String(b.rows.length)}語（${b.range}）`}</h3>
          <table className="cmp-words cmp-tag-usage-table">
            <thead>
              <tr>
                <th>語</th>
                <th>件数</th>
                <th>全体の割合</th>
                {hasPopular && <th>人気求人での割合</th>}
                {hasPopular && <th>差(pt)</th>}
              </tr>
            </thead>
            <tbody>
              {b.rows.slice(0, TABLE_ROWS_PER_BAND).map((r) => (
                <tr key={r.word}>
                  <td>{r.word}</td>
                  <td>{r.n_tag}</td>
                  <td>{fixed(r.share_pct, 1)}%</td>
                  {hasPopular && <td>{r.popular_share_pct === null ? MISSING : `${fixed(r.popular_share_pct, 1)}%`}</td>}
                  {hasPopular && <td>{r.gap_pt === null ? MISSING : signed(r.gap_pt)}</td>}
                </tr>
              ))}
              {b.rows.length === 0 && (
                <tr>
                  <td colSpan={cols}>該当する語はありません</td>
                </tr>
              )}
              {b.rows.length > TABLE_ROWS_PER_BAND && (
                <tr>
                  <td colSpan={cols}>ほか {b.rows.length - TABLE_ROWS_PER_BAND} 語</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      ))}
      <p className="cmp-note cmp-word-basis">
        母数：全体 {usage.n} 件 / 人気求人 {usage.popular_n} 件（人気または超人気の表示がある求人）。件数1〜2の語（
        {usage.minor_word_count}語）は帯に入れていません。差は人気求人での割合−全体の割合。
      </p>
      {!hasPopular && <p className="cmp-note cmp-word-basis">{POPULARITY_ONLY_SP}</p>}
    </>
  );
}
