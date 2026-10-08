import { useMemo, useState } from 'react';
import type { JobCopyRecord } from './data';
import { MIN_RATE_DAYS, OVERVIEW_WINDOW_DAYS, comparableRate, overviewRange, overviewRows, sortOverview } from './overviewModel';
import { InfoTip } from './InfoTip';
import type { OverviewRow, OverviewSort } from './overviewModel';
import { daysBetween, formatDay, formatPerDay, formatYen } from './timelineModel';
import type { BillingEntry } from './timelineModel';
import { DEMO_BILLING_LABEL, DUMMY_BILLING_LABEL, DUMMY_BILLING_NOTE } from './dummyBilling';
import './timeline.css';

const sortLabels: Record<OverviewSort, string> = {
  source: '一覧と同じ順',
  lastChange: '直近の変更日が新しい順',
  afterPerDay: '変更後の1日あたり応募が多い順',
  beforePerDay: '変更前の1日あたり応募が多い順',
  billing: '課金合計が多い順',
};

function MiniCalendar({ row, range }: { row: OverviewRow; range: { start: string; end: string } }) {
  const total = daysBetween(range.start, range.end) + 1;
  const x = (date: string) => Math.max(0, Math.min(240, daysBetween(range.start, date) / total * 240));
  const max = Math.max(1, ...row.weeks.map(week => week.count));
  return <svg viewBox="0 0 240 34" role="img" aria-label={`${row.title}の週ごとの応募と変更日`} preserveAspectRatio="none">
    <line x1="0" y1="33" x2="240" y2="33" stroke="#cbd5e1" strokeWidth="1" />
    {row.weeks.map(week => {
      const left = x(week.start); const width = Math.max(1.5, x(week.end) - left - 1);
      const height = week.count / max * 26;
      return <rect key={week.start} x={left} y={33 - height} width={width} height={height} fill="#a36816"><title>{`${formatDay(week.start)}の週: ${String(week.count)}件`}</title></rect>;
    })}
    {row.changeDates.map(date => <line key={date} x1={x(date)} x2={x(date)} y1="0" y2="33" stroke="#b54708" strokeWidth="1.5" strokeDasharray="3 2"><title>{`変更 ${formatDay(date)}`}</title></line>)}
  </svg>;
}

/** Real total and the dummy total side by side; the dummy is always labelled and never added in. */
export function overviewBillingText(row: OverviewRow): string {
  const real = !row.billingConnected ? null : row.billingOverlapping ? '期間が重なる課金あり' : row.billingYen === null ? '金額の記載なし' : `${row.billingFictional ? `${DEMO_BILLING_LABEL} ` : ''}${formatYen(row.billingYen)}${row.billingMissingAmount ? '（記載なしの期間あり）' : ''}`;
  const dummy = row.dummyBillingYen === null ? null : `${DUMMY_BILLING_LABEL} ${formatYen(row.dummyBillingYen)}`;
  if (real && dummy) return `${real} ／ ${dummy}`;
  return real ?? dummy ?? '課金データなし';
}

function rateText(rate: OverviewRow['before']) {
  if (!rate) return '—';
  if (!comparableRate(rate)) return <span className="jo-short">期間が短いため比べません<small>{rate.applications}件 / {rate.days}日</small></span>;
  return <>{formatPerDay(rate.perDay)}<small>{rate.applications}件 / {rate.days}日</small></>;
}

export interface JobOverviewProps {
  records: readonly JobCopyRecord[];
  /** Billing rows from outside the snapshot, by job id (kept in the browser only). */
  billing?: Readonly<Record<string, readonly BillingEntry[]>> | undefined;
  onChoose: (job: JobCopyRecord) => void;
  now?: Date | undefined;
}

export function JobOverview({ records, billing, onChoose, now }: JobOverviewProps) {
  const [sort, setSort] = useState<OverviewSort>('source');
  const rows = useMemo(() => overviewRows(records, { billing, now }), [records, billing, now]);
  const range = useMemo(() => overviewRange(records, rows), [records, rows]);
  const sorted = useMemo(() => sortOverview(rows, sort), [rows, sort]);
  const byId = new Map(records.map(job => [job.id, job]));
  return <main className="jc-detail jo-overview" id="job-details" tabIndex={-1} aria-labelledby="job-overview-heading">
    <h1 id="job-overview-heading">求人の横断比較</h1>
    <div className="jc-muted">直近の変更の前後{OVERVIEW_WINDOW_DAYS}日間の1日あたり応募を並べています（HubSpot に記録された応募のみ）。<InfoTip className="jc-infotip-left" label="この表の見方">
      <p>並べて見るための表で、どの求人が良いかを決めるものではありません。</p>
      <p>変更の前後が{MIN_RATE_DAYS}日に満たないときは、1日あたりの数を比べず並び替えにも使いません。</p>
      <p>市場の動き（Indeed の求人数など）は月ごとのデータなので、この表には並べていません。求人名を選ぶと、タイムラインの「市場」の段で同じ時間軸で確認できます。</p>
    </InfoTip></div>
    <div className="jo-sort"><label>並び替え<select value={sort} onChange={event => { setSort(event.target.value as OverviewSort); }}>{(Object.keys(sortLabels) as OverviewSort[]).map(key => <option key={key} value={key}>{sortLabels[key]}</option>)}</select></label>
      {range && <span className="jc-muted">カレンダー：{formatDay(range.start)}〜{formatDay(range.end)}（棒は週ごとの応募、点線は変更日）</span>}</div>
    {!rows.length ? <p className="jc-notice">表示できる求人がありません。</p> : <div className="jo-table-scroll" role="region" aria-label="求人の横断比較の表" tabIndex={0}><table>
      <thead><tr><th scope="col">求人</th><th scope="col">応募と変更日</th><th scope="col">直近の変更日</th><th scope="col">変更の種類</th><th scope="col">変更前{OVERVIEW_WINDOW_DAYS}日の1日あたり応募</th><th scope="col">変更後{OVERVIEW_WINDOW_DAYS}日の1日あたり応募</th><th scope="col">課金合計</th></tr></thead>
      <tbody>{sorted.map(row => {
        const job = byId.get(row.jobId);
        return <tr key={row.jobId}>
          <th scope="row">{job ? <button type="button" className="jc-text-button" style={{ padding: 0 }} onClick={() => { onChoose(job); }}>{row.title}</button> : row.title}<small>{row.company} · {row.media}</small></th>
          <td className="jo-calendar">{range && <MiniCalendar row={row} range={range} />}</td>
          <td>{row.lastChange ? formatDay(row.lastChange) : '変更なし'}</td>
          <td className="jo-kinds">{row.kinds.length ? row.kinds.map(kind => <span key={kind}>{kind}</span>) : row.lastChange ? '判定できない変更' : '—'}</td>
          <td>{row.applicationsAvailable ? rateText(row.before) : '応募未取得'}</td>
          <td>{row.applicationsAvailable ? rateText(row.after) : '応募未取得'}</td>
          <td className="jo-billing">{overviewBillingText(row)}</td>
        </tr>;
      })}</tbody>
    </table></div>}
    {sorted.some(row => row.dummyBillingYen !== null) && <p className="jt-dummy-billing" role="note">課金合計の{DUMMY_BILLING_NOTE}</p>}
    {sorted.some(row => [row.before, row.after].some(rate => rate ? rate.days < OVERVIEW_WINDOW_DAYS : false)) && <p className="jc-muted jo-short-note">
      変更前・変更後が{OVERVIEW_WINDOW_DAYS}日に満たない求人は、ある日数分だけで1日あたりを数えています（各欄の「件 / 日」の日数）。変更後は応募を取得した日までです。</p>}
  </main>;
}
