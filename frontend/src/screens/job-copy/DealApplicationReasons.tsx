import type { DealRow } from './dealBoardModel';
import './applicant-reasons.css';

type Summary = NonNullable<DealRow['application_reasons']>;
export function DealApplicationReasons({ value, outOfScope = false, details = false }: { value: Summary | null | undefined; outOfScope?: boolean; details?: boolean }) {
  // 満了から8週を過ぎた終了済みの取引は応募を読んでいない（応募数と同じく対象外）。
  if (!value && outOfScope) return details ? null : <span className="db-application-reasons">応募動機：対象外</span>;
  if (!value) return <span className="db-application-reasons">応募動機：集計中</span>;
  const denominator = `応募動機を聞けた ${String(value.answered[0] ?? 0)}件／2026/09/14 以降の応募 ${String(value.total)}件`;
  if (!details) return <span className="db-application-reasons">
    {value.answered[0] ? <>多い応募動機：{value.top.map((choice, i) => <span key={i}>{i > 0 && ' ／ '}{choice.label}{choice.unknown && '（いまの選択肢にない値）'} {choice.counts[0]}件</span>)}</> : '応募動機の記録はまだありません'}
    <small>{denominator}</small>
  </span>;
  const table = (title: string, rows: Summary['motives'], columns: readonly string[], note: string) => <section>
    <h3>{title}</h3><p className="ar-selection-denominator">{note}</p>
    <div className="ar-table-scroll"><table className="ar-table ar-selection-table"><caption>{title}の選択件数</caption>
      <thead><tr><th scope="col">選択肢</th>{columns.map(label => <th key={label} scope="col">{label}</th>)}</tr></thead>
      <tbody>{rows.map((choice, i) => <tr key={i}><th scope="row">{choice.label}{choice.unknown && <small className="ar-note">（いまの選択肢にない値）</small>}</th>{choice.counts.map((count, index) => <td key={index}>{count}件</td>)}</tr>)}</tbody>
    </table></div>
  </section>;
  return <div className="db-reason-tables">
    {table('応募動機', value.motives, ['件数'], denominator)}
    {table('勤務条件の優先順位', value.priorities, ['①', '②', '③'], `①を聞けた応募 ${String(value.answered[1] ?? 0)}件・②${String(value.answered[2] ?? 0)}件・③${String(value.answered[3] ?? 0)}件／2026/09/14 以降の応募 ${String(value.total)}件`)}
  </div>;
}
