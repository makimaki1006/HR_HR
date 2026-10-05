import { useState } from 'react';
import type { JobCopyRecord } from './data';
import { comparePerformance, performanceRatios } from './hrhPerformanceModel';
import './job-analysis.css';

const number = (value: number | null, suffix = '') => value === null ? '算出不可・未取得' : `${value.toLocaleString('ja-JP', { maximumFractionDigits: 2 })}${suffix}`;
export function HrhPerformance({ job }: { job: JobCopyRecord }) {
  const data = job.hrhPerformance;
  const [before, setBefore] = useState(0); const [after, setAfter] = useState(1);
  if (job.media !== 'HRハッカー') return <section className="jc-analysis"><h2>課金・クリック実績</h2><p>この連携はHRハッカー求人が対象です。</p></section>;
  if (!data) return <section className="jc-analysis"><h2>課金・クリック実績</h2><p className="jc-notice">この求人の実績データは未接続です。クリック数・費用を0として扱いません。</p><p>HRハッカー求人ID {job.mediaJobId} と対象期間を使い、既存の実績取得データを対応させます。表示回数がなければクリック率は算出できません。</p></section>;
  const left = data.rows[before]; const right = data.rows[after];
  const comparison = left && right && before !== after ? comparePerformance(left, right) : null;
  return <section className="jc-analysis" aria-label="HRハッカー課金・クリック実績"><h2>課金・クリック実績</h2><p>HRハッカー求人ID：{data.job_id} · 取得：{data.captured_at}</p><p className="jc-notice">各行は媒体の対象期間の実績です。応募数は媒体報告値で、HubSpot応募総数とは別です。文面・画像・課金の変更による因果を示す値ではありません。</p>
    {data.rows.length > 0 && <p className="jc-analysis-scroll-hint">表を横にスクロールすると、応募数・応募単価まで確認できます。</p>}
    {!data.rows.length ? <p>対象期間の行がありません。</p> : <div className="jc-analysis-table" role="region" aria-label="期間別実績の横スクロール" tabIndex={0}><table><caption>期間別の実績（期間を合算せず表示）</caption><thead><tr>{['対象期間', '表示回数', 'クリック数', 'クリック率', '費用', 'クリック単価', '媒体応募数', '媒体応募単価'].map(label => <th key={label}>{label}</th>)}</tr></thead><tbody>{data.rows.map(row => { const ratios = performanceRatios(row); return <tr key={row.period_start}><th>{row.period_start}〜{row.period_end}</th><td>{number(row.impressions)}</td><td>{number(row.clicks)}</td><td>{number(ratios.ctr, '%')}</td><td>{number(row.cost_yen, '円')}</td><td>{number(ratios.cpc, '円')}</td><td>{number(row.applications)}</td><td>{number(ratios.cpa, '円')}</td></tr>; })}</tbody></table></div>}
    {data.rows.length >= 2 && <><div className="jc-analysis-controls"><label>比較元の実績期間<select value={before} onChange={event => { setBefore(Number(event.target.value)); }}>{data.rows.map((row, index) => <option value={index} key={row.period_start}>{row.period_start}〜{row.period_end}</option>)}</select></label><label>比較先の実績期間<select value={after} onChange={event => { setAfter(Number(event.target.value)); }}>{data.rows.map((row, index) => <option value={index} key={row.period_start}>{row.period_start}〜{row.period_end}</option>)}</select></label></div>{comparison ? <p aria-label="実績期間の比較">クリック率の差：{number(comparison.ctrDeltaPp, 'ポイント')} · 1日当たりクリック数の差：{number(comparison.clicksPerDayDelta, '件')}</p> : <p>異なる期間を選択してください。</p>}<p>期間の長さ、季節、課金額、媒体の配信条件も確認してください。掲載版との対応は未確認です。</p></>}
  </section>;
}
