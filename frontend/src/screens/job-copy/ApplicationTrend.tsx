import { useMemo } from 'react';
import { EChart } from '../../components/EChart';
import type { JobCopyRecord } from './data';
import { monthlyApplications, trendOption } from './marketChartModel';
import './job-analysis.css';

const amount = (value: number | null | undefined) => value == null ? '未取得' : value.toLocaleString('ja-JP', { maximumFractionDigits: 2 });
export function ApplicationTrend({ job }: { job: JobCopyRecord }) {
  const applications = useMemo(() => monthlyApplications(job), [job]);
  const total = job.overallApplications;
  return <section className="jc-analysis jc-internal-market" aria-label="この求人の応募推移"><h2>この求人の月別応募数</h2>
    {total && <p>取得済み応募：{amount(total.total)}件 · 応募日あり：{amount(total.total - total.missingDate)}件 · 応募日不明：{amount(total.missingDate)}件（グラフ対象外）</p>}
    {applications?.length ? <>
      <p>対象月：{applications[0]?.month}〜{applications.at(-1)?.month} · HubSpot求人レコードの応募日別集計。版との対応が不明な応募も含みます。</p>
      <EChart option={trendOption(applications.map(row => row.month), applications.map(row => row.count), 'この求人の応募数', '件', '#a36816', true, '日付付き応募の記録なし')} testId="jc-applications-monthly" renderer="svg" height={280} />
      <details><summary>月別応募数の数値を確認</summary><table><caption>応募日が分かる取得済み応募の月別集計</caption><thead><tr><th>対象月</th><th>応募数</th></tr></thead><tbody>{applications.map(row => <tr key={row.month}><th>{row.month}</th><td>{row.count === null ? '日付付き応募の記録なし' : `${amount(row.count)}件`}</td></tr>)}</tbody></table></details>
      <p>棒がない月はこの集計に日付付き応募がありません。未収集の応募や媒体上の全応募が0だったことを示すものではありません。取得：{total?.fetchedAt.slice(0, 10)}</p>
    </> : <p>{total?.total === 0 ? '取得済み応募は0件です。応募の対象月がないためグラフは表示しません。' : total && total.total === total.missingDate ? '取得済み応募の応募日がすべて不明のため、月別応募グラフは表示できません。' : '応募日別集計が未取得のため、月別応募グラフは表示できません。'} 応募数を観測日に割り当てません。</p>}
  </section>;
}
