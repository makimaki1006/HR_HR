import type { CompetitorMonth } from '../../generated/CompetitorMonth';
import type { JobCopyRecord } from './data';
import { applicationsNote, competitorEntryUrl, competitorRows, conditionsText, monthJa, salaryText } from './competitorHistoryModel';
import { OutLink } from './Pressable';

/** その月の記録の中身（件数・給与・よく出る言葉・条件）。出来事を開いたときに出す。 */
export function CompetitorMonthDetails({ entry }: { entry: CompetitorMonth }) {
  const { count, salary } = entry;
  return <div className="lo-competitor-month" data-testid="competitor-month">
    <dl className="lo-application-fields">
      <div><dt>競合の件数</dt><dd>{count.total.toLocaleString('ja-JP')}件（給与が読めた求人 {count.salary_parsed.toLocaleString('ja-JP')}件{count.popular !== null ? `・人気の表示 ${count.popular.toLocaleString('ja-JP')}件` : ''}{count.super_popular !== null ? `・とても人気の表示 ${count.super_popular.toLocaleString('ja-JP')}件` : ''}）</dd></div>
      <div><dt>競合の給与（下限）</dt><dd>中央値 {salaryText(salary.all.min_median, salary.unit)}{count.salary_median_n !== null ? `（${count.salary_median_n.toLocaleString('ja-JP')}件から）` : ''} ／ 平均 {salaryText(salary.all.min_mean, salary.unit)}{entry.conditions.salary_converted ? '（CSV の給与を月給に換算した値）' : ''}</dd></div>
      <div><dt>競合の給与（上限）</dt><dd>中央値 {salaryText(salary.all.max_median, salary.unit)} ／ 平均 {salaryText(salary.all.max_mean, salary.unit)}</dd></div>
      <div><dt>このときの自社の給与</dt><dd>{entry.own_salary ? `${salaryText(entry.own_salary.min, salary.unit)} 〜 ${salaryText(entry.own_salary.max, salary.unit)}` : '記録なし（求人の本文から読めませんでした）'}</dd></div>
      <div><dt>よく出る言葉</dt><dd>{entry.keywords_top5.length ? entry.keywords_top5.map(([word, n]) => `${word}（${String(n)}件）`).join('、') : '記録なし'}</dd></div>
      <div><dt>調べた条件</dt><dd>{conditionsText(entry)}</dd></div>
      <div><dt>記録した人</dt><dd>{entry.by}</dd></div>
    </dl>
  </div>;
}

/**
 * 「競合の推移」。競合調査の入口と、月ごとの記録の表。
 * 数字を並べるだけで、高い・低いなどの判定は出さない。条件が前の月と違う月には印を付ける。
 */
export function CompetitorHistory({ job }: { job: JobCopyRecord }) {
  const rows = competitorRows(job);
  const url = competitorEntryUrl(job);
  return <section className="lo-card lo-competitor" aria-label="競合の推移" data-testid="competitor-history">
    <div className="lo-head"><h2>競合の推移</h2><span>競合調査で記録した月ごとの数字です。</span>{url && <span className="jc-no-print"><OutLink href={url} label="この求人で競合調査をする（新しいタブ）" /></span>}</div>
    {rows.length === 0 ? <p className="lo-note" role="status">まだ競合調査の記録はありません</p> : <>
      <div className="lo-competitor-scroll"><table className="lo-cost-table lo-competitor-table">
        <thead><tr><th scope="col">月</th><th scope="col">競合の件数</th><th scope="col">競合の給与の中央値（下限）</th><th scope="col">自社の給与（下限）</th><th scope="col">自社の応募</th><th scope="col">調べた条件</th></tr></thead>
        <tbody>{rows.map(row => <tr key={row.month} data-testid={`competitor-row-${row.month}`}>
          <th scope="row">{monthJa(row.month)}</th>
          <td>{row.total.toLocaleString('ja-JP')}件</td>
          <td>{salaryText(row.median, row.unit)}{row.median !== null && row.salaryN !== null && <small className="lo-competitor-n">（{row.salaryN.toLocaleString('ja-JP')}件から）</small>}{row.median !== null && row.converted && <small className="lo-competitor-n">月給に換算した値</small>}</td>
          <td>{salaryText(row.own, row.unit)}</td>
          <td>{row.applications === null ? '—' : `${row.applications.toLocaleString('ja-JP')}件`}{row.applications !== null && row.partial && <small className="lo-competitor-n">（途中）</small>}</td>
          <td className="lo-competitor-cond">{row.changed.length ? <span className="lo-mark lo-mark-caution" title={conditionsText(row.entry)}>前の月と違う：{row.changed.join('・')}</span> : <span title={conditionsText(row.entry)}>{rows[0] === row ? '最初の記録' : '前の月と同じ'}</span>}</td>
        </tr>)}</tbody>
      </table></div>
      <p className="lo-note">自社の給与は、その月に記録したときの求人の給与です（AirWork・リクロジの求人は本文から給与を読まないため空になります）。{applicationsNote(job, rows)}条件が前の月と違う月は、数字をそのまま比べられません。</p>
    </>}
  </section>;
}
