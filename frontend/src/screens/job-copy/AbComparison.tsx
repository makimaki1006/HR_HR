import { useState } from 'react';
import type { CopyVersion, JobCopyRecord } from './data';
import { compareDistributions } from './applicantCompositionModel';
import { publishedVariants, variantCount, variantDistribution, compareMetricPeriods } from './abComparisonModel';
import type { AbScope } from './abComparisonModel';
import { performanceRatios } from './hrhPerformanceModel';
import type { HrhPerformanceRow } from './hrhPerformanceModel';
import { ImageGallery } from './ImageGallery';
import { imagesByVersion } from './images';
import { reasonSourceLabels } from './applicantReasonsModel';
import { unmatchedApplicationCount } from './applicationCountsModel';
import './ab-comparison.css';

const number = (value: number | null | undefined, unit = '') => value === null || value === undefined ? '未取得・算出不可' : `${value.toLocaleString('ja-JP', { maximumFractionDigits: 2 })}${unit}`;
const dimensions = [{ id: 'gender', label: '性別' }, { id: 'age', label: '年代' }, { id: 'prefecture', label: '都道府県' }, { id: 'municipality', label: '市区町村' }] as const;
function MetricValues({ row }: { row: HrhPerformanceRow | undefined }) {
  if (!row) return <p>課金・クリック実績は未接続、または期間未選択です。</p>;
  const ratios = performanceRatios(row);
  return <dl className="ab-values">{[
    ['表示数', number(row.impressions, '回')], ['クリック数', number(row.clicks, '回')], ['CTR', number(ratios.ctr, '%')],
    ['費用', number(row.cost_yen, '円')], ['媒体応募数', number(row.applications, '件')], ['CPC', number(ratios.cpc, '円')], ['CPA', number(ratios.cpa, '円')],
  ].map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>;
}
function Variant({ label, job, version, scope, metric, onVersion, onMetric }: {
  label: string; job: JobCopyRecord; version: CopyVersion | undefined; scope: AbScope; metric: string;
  onVersion: (value: string) => void; onMetric: (value: string) => void;
}) {
  const row = job.hrhPerformance?.rows.find(item => item.period_start === metric);
  const reasons = job.applicantReasons;
  const texts = reasons?.available ? reasons.items.filter(item => scope === 'record' || item.versionId === version?.id) : [];
  return <section className="ab-variant" aria-label={`${label}求人の比較内容`}>
    <h3>{label}：{job.title}</h3><p>{job.company} · {job.location} · {job.media}</p><p>媒体求人ID：{job.mediaJobId} / HubSpot ID：{job.hubspotId ?? '未接続'}</p>
    {job.hubspotUrl && <a href={job.hubspotUrl} target="_blank" rel="noreferrer">{label}のHubSpot求人を開く</a>}
    <label>{label}の本文観測版<select value={version?.id ?? ''} onChange={event => { onVersion(event.target.value); }}><option value="">観測版を選択</option>{publishedVariants(job).map(item => <option key={item.id} value={item.id}>{item.label} · {item.observedAt.slice(0, 10)}</option>)}</select></label>
    <p>応募数（{scope === 'record' ? '求人レコード全体' : '選択版の確定＋推定対応'}）：<strong>{number(variantCount(job, version, scope), '件')}</strong></p>
    <p>{scope === 'record' ? `応募集計取得日時：${job.overallApplications?.fetchedAt ?? '未取得'}` : `確定 ${number(version?.applications?.confirmed, '件')} / 推定 ${number(version?.applications?.estimated, '件')}`}</p>
    <p>求人全体の版対応不明：{number(unmatchedApplicationCount(job), '件')}（選択版の応募数には含めません）</p>
    {version ? <><ImageGallery title={`${label}の掲載画像`} images={version.images ?? imagesByVersion[version.id]} />{version.historicalImageBytesAvailable === false && <p className="jc-notice">当時の画像原本は未保存です。</p>}<details><summary>{label}の求人本文を全文確認</summary><pre className="jc-body">{version.body}</pre><p>{version.note}</p></details></> : <p>掲載を観測した本文は未取得です。受信版・AI案は比較対象にしていません。</p>}
    <label>{label}の課金実績期間<select value={metric} onChange={event => { onMetric(event.target.value); }}><option value="">実績期間を選択</option>{job.hrhPerformance?.rows.map(item => <option key={item.period_start} value={item.period_start}>{item.period_start}〜{item.period_end}</option>)}</select></label>
    <MetricValues row={row} />
    <details className="jc-no-print"><summary>{label}の応募理由（内部閲覧）</summary><p>表示対象：{texts.length}記述。複数出典を含み、応募人数・回答率とは異なります。{reasons?.truncated ? '取得上限による一部表示です。' : ''}</p>
      {!reasons?.available ? <p>応募理由は未取得です。</p> : <><p>版対応不明の表示対象：{reasons.items.filter(item => item.versionId === null).length}記述。選択版の反応には割り当てません。</p>{texts.map(item => <details key={item.id}><summary>原記録を開く · {reasonSourceLabels[item.sourceProperty] ?? item.sourceProperty} · {item.applicationDate ?? '応募日不明'}</summary><p>個人情報を含む可能性のある原記録です。版対応：{item.versionId ?? '不明'}</p><blockquote>{item.text}</blockquote></details>)}</>}
    </details>
  </section>;
}

export function AbComparison({ job, records }: { job: JobCopyRecord; records: JobCopyRecord[] }) {
  const [otherId, setOtherId] = useState('');
  const [versionA, setVersionA] = useState(publishedVariants(job).at(-1)?.id ?? '');
  const [versionB, setVersionB] = useState('');
  const [metricA, setMetricA] = useState(''); const [metricB, setMetricB] = useState('');
  const [scope, setScope] = useState<AbScope>('record');
  const [confirmed, setConfirmed] = useState(false); const [name, setName] = useState(''); const [hypothesis, setHypothesis] = useState('');
  const other = records.find(item => item.id === otherId && item.id !== job.id);
  const a = publishedVariants(job).find(item => item.id === versionA); const b = other ? publishedVariants(other).find(item => item.id === versionB) : undefined;
  const rowA = job.hrhPerformance?.rows.find(item => item.period_start === metricA); const rowB = other?.hrhPerformance?.rows.find(item => item.period_start === metricB);
  const periods = rowA && rowB ? compareMetricPeriods(rowA, rowB) : null;
  function chooseB(id: string) { const chosen = records.find(item => item.id === id); setOtherId(id); setVersionB(chosen ? publishedVariants(chosen).at(-1)?.id ?? '' : ''); setMetricB(''); setConfirmed(false); }
  return <section className="ab-comparison" aria-label="2求人のA/B比較"><h2>2つの求人をA/B比較する</h2><p>求人IDや本文が違っても、同じ募集の比較対象として組み合わせられます。元のレコード・履歴・応募はそれぞれ保持します。</p>
    <div className="ab-controls jc-no-print"><label>比較グループ名<input value={name} maxLength={120} placeholder="例：配送募集・画像の訴求比較" onChange={event => { setName(event.target.value); }} /></label><label>検証したい仮説<textarea value={hypothesis} maxLength={1000} rows={2} onChange={event => { setHypothesis(event.target.value); }} /></label>
      <label>Bとして比較する求人<select value={otherId} onChange={event => { chooseB(event.target.value); }}><option value="">別の求人を選択</option>{records.filter(item => item.id !== job.id).map(item => <option key={item.id} value={item.id}>{item.company} · {item.title} · {item.mediaJobId}</option>)}</select></label>
      <label><input type="checkbox" checked={confirmed} disabled={!other} onChange={event => { setConfirmed(event.target.checked); }} />同じ募集として比較する組み合わせを確認した</label>
      <label>応募の比較範囲<select value={scope} onChange={event => { setScope(event.target.value === 'version' ? 'version' : 'record'); }}><option value="record">求人レコード全体（取得済み応募）</option><option value="version">選択した本文観測版（確定・推定対応）</option></select></label>
      <p className="jc-muted">比較設定はこの画面内のMOCです。再読み込み・求人切替で消え、共有保存は未接続です。</p>
    </div>
    {!other ? <p role="status">B求人を選ぶと、A/Bの本文・画像・応募結果が並びます。候補は読み込み済み求人です。</p> : <>
      <h3>{name || '名称未設定の比較グループ'}</h3>{hypothesis && <p>仮説：{hypothesis}</p>}
      {!confirmed && <p className="jc-notice" role="status">組み合わせ未確認。比較対象の募集内容・取引先・勤務地を確認してください。</p>}
      {(job.company !== other.company || job.location !== other.location || job.media !== other.media) && <p className="jc-notice">取引先・勤務地・媒体に違いがあります。募集条件や露出条件の違いも含む比較です。</p>}
      <p className="jc-notice">{scope === 'record' ? '応募者構成・理由は取得済みの求人全体です。選択した文面や課金期間の成果ではありません。' : '応募者構成は選択版に対応する確定・推定応募です。版対応不明は除外し、理由も版対応が確認できる記述のみ表示します。'} 媒体実績期間と本文版の対応は未確認です。</p>
      <div className="ab-pair"><Variant label="A" job={job} version={a} scope={scope} metric={metricA} onVersion={setVersionA} onMetric={setMetricA} /><Variant label="B" job={other} version={b} scope={scope} metric={metricB} onVersion={setVersionB} onMetric={setMetricB} /></div>
      <section aria-label="A/B実績期間の比較"><h3>課金実績期間の比較</h3>{periods ? <><p>A：{rowA?.period_start}〜{rowA?.period_end}（{periods.daysA}日） / B：{rowB?.period_start}〜{rowB?.period_end}（{periods.daysB}日） · 重なる日数：{periods.overlapDays}日</p><p>CTR差（B−A）：{number(periods.ctrDeltaPp, 'ポイント')} / クリック→媒体応募率：A {number(periods.cvrA, '%')}・B {number(periods.cvrB, '%')}</p><p>期間の集計値を日割りで補間しません。露出配分・予算・曜日・市場環境も確認してください。</p></> : <p>両求人の実績期間を選ぶと比較できます。HubSpot応募数を媒体の転換率計算には使いません。</p>}</section>
      <section aria-label="A/B応募者構成の比較"><h3>応募者構成（件数・割合）</h3>{dimensions.map(dimension => {
        const rows = compareDistributions(variantDistribution(job, a, scope, dimension.id), variantDistribution(other, b, scope, dimension.id));
        return <details key={dimension.id}><summary>{dimension.label}のA/B比較</summary>{rows === null ? <p>片方または両方の属性が未取得です。</p> : <div className="ab-table-scroll" tabIndex={0} role="region" aria-label={`${dimension.label}比較表の横スクロール`}><table><caption>{dimension.label} · {scope === 'record' ? '求人全体' : '選択版対応'} · 割合の分母は属性不明を含む応募</caption><thead><tr><th>区分</th><th>A件数</th><th>A割合</th><th>B件数</th><th>B割合</th><th>B−A</th></tr></thead><tbody>{rows.map(row => <tr key={row.category}><th scope="row">{row.category}</th><td>{row.beforeCount}</td><td>{number(row.beforePercentage, '%')}</td><td>{row.afterCount}</td><td>{number(row.afterPercentage, '%')}</td><td>{number(row.deltaPp, 'ポイント')}</td></tr>)}</tbody></table>{!rows.length && <p>両方の対象応募は0件です。割合は算出できません。</p>}</div>}</details>;
      })}</section>
      <p>露出の無作為な振り分けは行っていません。同時掲載・時期をずらした掲載のどちらも観測比較として扱い、自動で勝者や変更効果を断定しません。</p>
    </>}
  </section>;
}
