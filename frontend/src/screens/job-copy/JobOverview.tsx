import { useMemo, useState } from 'react';
import type { JobCopyRecord } from './data';
import { MIN_RATE_DAYS, OVERVIEW_WINDOW_DAYS, comparableRate, overviewRange, overviewRows, sortOverview } from './overviewModel';
import { InfoTip } from './InfoTip';
import type { ChangeWindow, OverviewRow, OverviewSort } from './overviewModel';
import { daysBetween, formatDay, formatPerDay, formatYen } from './timelineModel';
import type { BillingEntry } from './timelineModel';
import { DEMO_BILLING_LABEL, DUMMY_BILLING_LABEL } from './dummyBilling';
import './timeline.css';

const sortLabels: Record<OverviewSort, string> = {
  source: '一覧と同じ順',
  lastChange: '直近の変化が新しい順（後の取得日）',
  afterPerDay: '変化後（後の取得日の翌日から）の1日あたり応募が多い順',
  beforePerDay: '変化前（前の取得日の前日まで）の1日あたり応募が多い順',
  billing: '実際の課金合計が多い順',
};

function MiniCalendar({ row, range }: { row: OverviewRow; range: { start: string; end: string } }) {
  const total = daysBetween(range.start, range.end) + 1;
  const x = (date: string) => Math.max(0, Math.min(240, daysBetween(range.start, date) / total * 240));
  const max = Math.max(1, ...row.weeks.map(week => week.count));
  return <svg viewBox="0 0 240 34" role="img" aria-label={`${row.title}の週ごとの応募と、変化が見つかった取得日の間`} preserveAspectRatio="none">
    <line x1="0" y1="33" x2="240" y2="33" stroke="#cbd5e1" strokeWidth="1" />
    {row.weeks.map(week => {
      const left = x(week.start); const width = Math.max(1.5, x(week.end) - left - 1);
      const height = week.count / max * 26;
      return <rect key={week.start} x={left} y={33 - height} width={width} height={height} fill="#a36816"><title>{`${formatDay(week.start)}の週: ${String(week.count)}件`}</title></rect>;
    })}
    {row.changes.map(change => change.exact
      ? <line key={change.to} x1={x(change.to)} x2={x(change.to)} y1="0" y2="33" stroke="#b54708" strokeWidth="1.5" strokeDasharray="3 2"><title>{changeText(change)}</title></line>
      : <rect key={`${change.from}-${change.to}`} className="jo-change-span" x={x(change.from)} y="0" width={Math.max(1.5, x(change.to) - x(change.from))} height="33" fill="#b54708" fillOpacity="0.18" stroke="#b54708" strokeWidth="1" strokeDasharray="3 2"><title>{changeText(change)}</title></rect>)}
  </svg>;
}

/** 「取得日2026/07/01〜2026/08/20の間に変化」 (or the publication day when it is known). */
export function changeText(change: ChangeWindow): string {
  return change.exact ? `${formatDay(change.to)}に掲載が変わりました（媒体の掲載日時）` : `取得日${formatDay(change.from)}〜${formatDay(change.to)}の間に変化`;
}

/**
 * The real billing total only. The dummy billing is never added up here: a job with only the dummy
 * billing says there is no real billing data.
 */
export function overviewBillingText(row: OverviewRow): string {
  if (row.billingConflict) return 'HRハッカーの実績と課金CSVで同じ期間の課金が重なっています（合計しません）';
  if (!row.billingConnected) return row.hasDummyBilling ? `実際の課金データなし（${DUMMY_BILLING_LABEL}は合計しません）` : '課金データなし（0円という意味ではありません）';
  const real = row.billingOverlapping ? '期間が重なる課金あり（合計しません）' : row.billingYen === null ? '金額の記載なし' : `${row.billingFictional ? `${DEMO_BILLING_LABEL} ` : ''}${formatYen(row.billingYen)}${row.billingMissingAmount ? '（記載なしの期間あり）' : ''}`;
  return row.hasDummyBilling ? `${real}（${DUMMY_BILLING_LABEL}は合計に入れていません）` : real;
}

function rateText(rate: OverviewRow['before']) {
  if (!rate) return '—';
  if (!comparableRate(rate)) return <span className="jo-short">期間が短いため比べません<small>{rate.days === 0 ? '内容が分かっている日がありません' : `${String(rate.applications)}件 / ${String(rate.days)}日`}</small></span>;
  return <>{formatPerDay(rate.perDay)}<small>{rate.applications}件 / {rate.days}日</small></>;
}

export interface JobOverviewProps {
  records: readonly JobCopyRecord[];
  /** Billing rows from outside the snapshot, by job id (kept in the browser only). */
  billing?: Readonly<Record<string, readonly BillingEntry[]>> | undefined;
  onChoose: (job: JobCopyRecord) => void;
  now?: Date | undefined;
  /** Show the dummy billing (仮の課金データ). It is never added up or used for sorting. */
  showDummyBilling?: boolean | undefined;
}

export function JobOverview({ records, billing, onChoose, now, showDummyBilling }: JobOverviewProps) {
  const [sort, setSort] = useState<OverviewSort>('source');
  const rows = useMemo(() => overviewRows(records, { billing, now, dummyBilling: showDummyBilling }), [records, billing, now, showDummyBilling]);
  const range = useMemo(() => overviewRange(records, rows), [records, rows]);
  const sorted = useMemo(() => sortOverview(rows, sort), [rows, sort]);
  const byId = new Map(records.map(job => [job.id, job]));
  return <main className="jc-detail jo-overview" id="job-details" tabIndex={-1} aria-labelledby="job-overview-heading">
    <h1 id="job-overview-heading">求人の横断比較</h1>
    <div className="jc-muted">直近の変化が見つかった2つの取得日について、前の取得日の前日までの{OVERVIEW_WINDOW_DAYS}日間と、後の取得日の翌日からの{OVERVIEW_WINDOW_DAYS}日間の1日あたり応募を並べています（HubSpot に記録された応募のみ）。<InfoTip className="jc-infotip-left" label="この表の見方">
      <p>掲載が変わった日は分かりません。前の取得日と後の取得日の間のどこかで変わったとして、その間の応募は前後どちらにも入れていません。応募は日付だけで記録されていて、2つの取得日も取得した時刻の前後で変わった可能性があるため、取得日当日の応募も入れていません。</p>
      <p>複数の求人に関連する応募は、どの求人の応募か決められないため数えていません（見分ける情報がある場合）。</p>
      <p>並べて見るための表で、どの求人が良いかを決めるものではありません。</p>
      <p>「多い応募理由」は、HubSpot に記録された応募理由を分類し、件数の多い2つを並べたものです（選択された分類と、文から言葉で推定した分類の合計。nは応募理由の記録がある応募の件数）。「記録なし」は応募理由の記録が無いこと、「未取得」は応募理由を取得していないことです。</p>
      <p>変更の前後が{MIN_RATE_DAYS}日に満たないときは、1日あたりの数を比べず並び替えにも使いません。</p>
      <p>市場の動き（Indeed の求人数など）は月ごとのデータなので、この表には並べていません。求人名を選ぶと、タイムラインの「市場」の段で同じ時間軸で確認できます。</p>
    </InfoTip></div>
    <div className="jo-sort"><label>並び替え<select value={sort} onChange={event => { setSort(event.target.value as OverviewSort); }}>{(Object.keys(sortLabels) as OverviewSort[]).map(key => <option key={key} value={key}>{sortLabels[key]}</option>)}</select></label>
      {range && <span className="jc-muted">カレンダー：{formatDay(range.start)}〜{formatDay(range.end)}（棒は週ごとの応募、色の付いた帯は変化が見つかった取得日の間）</span>}</div>
    <p className="jo-sort-note" role="note">並び順は数の大小で並べただけです。応募が増えた・減った理由を示すものではありません。</p>
    {!rows.length ? <p className="jc-notice">表示できる求人がありません。</p> : <div className="jo-table-scroll" role="region" aria-label="求人の横断比較の表" tabIndex={0}><table>
      <thead><tr><th scope="col">求人</th><th scope="col">応募と変化</th><th scope="col">直近の変化</th><th scope="col">変化の種類</th><th scope="col">前の取得日の前日までの{OVERVIEW_WINDOW_DAYS}日の1日あたり応募</th><th scope="col">後の取得日の翌日からの{OVERVIEW_WINDOW_DAYS}日の1日あたり応募</th><th scope="col">実際の課金合計</th><th scope="col">多い応募理由</th></tr></thead>
      <tbody>{sorted.map(row => {
        const job = byId.get(row.jobId);
        return <tr key={row.jobId}>
          <th scope="row">{job ? <button type="button" className="jc-text-button" style={{ padding: 0 }} onClick={() => { onChoose(job); }}>{row.title}</button> : row.title}<small>{row.company} · {row.media}</small></th>
          <td className="jo-calendar">{range && <MiniCalendar row={row} range={range} />}</td>
          <td>{row.lastChange ? changeText(row.lastChange) : '変化は見つかっていません'}</td>
          <td className="jo-kinds">{row.kinds.length ? row.kinds.map(kind => <span key={kind}>{kind}</span>) : '—'}</td>
          <td>{row.applicationsAvailable ? rateText(row.before) : '応募未取得'}</td>
          <td>{row.applicationsAvailable ? rateText(row.after) : '応募未取得'}</td>
          <td className="jo-billing">{overviewBillingText(row)}</td>
          <td className="jo-reasons">{row.reasonText}</td>
        </tr>;
      })}</tbody>
    </table></div>}
    {sorted.some(row => row.hasDummyBilling) && <p className="jt-dummy-billing" role="note">{DUMMY_BILLING_LABEL}は架空の金額です。この表の課金合計にも並び替えにも使っていません。金額は各求人のタイムラインの「課金」の段で「ダミー」と付けて表示します。</p>}
    {sorted.some(row => [row.before, row.after].some(rate => rate ? rate.days < OVERVIEW_WINDOW_DAYS : false)) && <p className="jc-muted jo-short-note">
      前後が{OVERVIEW_WINDOW_DAYS}日に満たない求人は、ある日数分だけで1日あたりを数えています（各欄の「件 / 日」の日数）。後の期間は、その内容を最後に取得した日か、応募を取得した日までです。</p>}
  </main>;
}
