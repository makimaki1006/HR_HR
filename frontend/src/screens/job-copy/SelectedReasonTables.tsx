import type { ApplicationRow, JobCopyRecord } from './data';
import { MOTIVE, PRIORITIES, selectedCounts } from './selectedReasonsModel';
import './applicant-reasons.css';

export function SelectedReasonTables({ job, rows = job.applicationRows ?? [] }: { job: JobCopyRecord; rows?: readonly ApplicationRow[] }) {
  const definitions = job.overallApplications?.selectionOptions;
  if (!definitions || !job.applicationRows) return <p>選択式の応募理由は未取得です。</p>;
  const total = job.overallApplications?.total ?? job.applicationRows.length;
  const table = (title: string, properties: readonly string[], labels: readonly string[]) => {
    const options = definitions[properties[0] ?? ''];
    if (!options?.length) return <section key={title}><h3>{title}</h3><p>選択肢を読み込めませんでした。</p></section>;
    const results = properties.map(property => selectedCounts(rows, property));
    const denominator = properties[0] === MOTIVE
      ? `応募動機を聞けた応募 ${String(results[0]?.answered ?? 0)}件／この求人の全応募 ${String(total)}件`
      : `①を聞けた応募 ${String(results[0]?.answered ?? 0)}件・②${String(results[1]?.answered ?? 0)}件・③${String(results[2]?.answered ?? 0)}件／この求人の全応募 ${String(total)}件`;
    return <section key={title}><h3>{title}</h3><p className="ar-selection-denominator">{denominator}</p>
      <div className="ar-table-scroll"><table className="ar-table ar-selection-table"><caption>{title}の選択件数</caption>
        <colgroup><col />{properties.map(property => <col key={property} className="ar-selection-count" />)}</colgroup>
        <thead><tr><th scope="col">選択肢</th>{labels.map(label => <th scope="col" key={label}>{label}</th>)}</tr></thead>
        <tbody>{options.map(option => <tr key={option.value}><th scope="row">{option.label}</th>{properties.map((property, index) => {
          const own = definitions[property]?.find(item => item.label === option.label);
          return <td key={property}>{results[index]?.counts.get(own?.value ?? option.value) ?? 0}件</td>;
        })}</tr>)}</tbody>
      </table></div>
    </section>;
  };
  return <>{table('応募動機', [MOTIVE], ['件数'])}{table('勤務条件の優先順位', PRIORITIES, ['①', '②', '③'])}</>;
}
