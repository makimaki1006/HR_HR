import { useMemo } from 'react';
import { EChart } from '../../components/EChart';
import type { JobCopyRecord } from './data';
import { monthlyApplications, trendOption } from './marketChartModel';
import { AssumptionsNote } from './AssumptionsNote';
import { formatDateJst } from './format';
import './job-analysis.css';

const amount = (value: number | null | undefined) => value == null ? '未取得' : value.toLocaleString('ja-JP', { maximumFractionDigits: 2 });
export function ApplicationTrend({ job }: { job: JobCopyRecord }) {
  const applications = useMemo(() => monthlyApplications(job), [job]);
  const total = job.overallApplications;
  return <section className="jc-analysis jc-internal-market" aria-label="この求人の応募推移"><h2>この求人の月別応募数</h2>
    {total && <p>取得済み応募：{amount(total.total)}件 · 応募日あり：{amount(total.total - total.missingDate)}件 · 応募日不明：{amount(total.missingDate)}件（グラフ対象外）</p>}
    {applications?.length ? <>
      <p>対象月：{formatDateJst(applications[0]?.month)}〜{formatDateJst(applications.at(-1)?.month)}</p>
      <EChart option={trendOption(applications.map(row => formatDateJst(row.month)), applications.map(row => row.count), 'この求人の応募数', '件', '#a36816', true, '日付付き応募の記録なし')} testId="jc-applications-monthly" renderer="svg" height={280} />
      <details><summary>月別応募数の数値を確認</summary><table><caption>応募日が分かる取得済み応募の月別集計</caption><thead><tr><th>対象月</th><th>応募数</th></tr></thead><tbody>{applications.map(row => <tr key={row.month}><th>{formatDateJst(row.month)}</th><td>{row.count === null ? '日付付き応募の記録なし' : `${amount(row.count)}件`}</td></tr>)}</tbody></table></details>
      <AssumptionsNote summary={`HubSpotに記録された応募を、応募日で月別に数えています（どの版への応募か不明な応募も含みます）。取得日：${formatDateJst(total?.fetchedAt, '不明')}`} items={['棒がない月は、日付の付いた応募記録が無い月です。まだ集めていない応募や、媒体上の応募が0件だったことを示すものではありません。']} />
    </> : <p>{total?.total === 0 ? '取得済み応募は0件です。応募の対象月がないためグラフは表示しません。' : total && total.total === total.missingDate ? '取得済み応募の応募日がすべて不明のため、月別応募グラフは表示できません。' : '応募日別集計が未取得のため、月別応募グラフは表示できません。'} 応募数を取得日に割り当てません。</p>}
  </section>;
}
