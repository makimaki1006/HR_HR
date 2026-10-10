import { useState } from 'react';
import type { CopyVersion, JobCopyRecord } from './data';
import { compareDistributions } from './applicantCompositionModel';
import { publishedVariants, variantCount, variantDistribution, compareMetricPeriods } from './abComparisonModel';
import type { AbScope } from './abComparisonModel';
import { performanceRatios } from './hrhPerformanceModel';
import type { HrhPerformanceRow } from './hrhPerformanceModel';
import { ImageGallery } from './ImageGallery';
import { imagesByVersion } from './images';
import { APPLICATION_TEXT_SOURCES, reasonSourceLabels } from './applicantReasonsModel';
import { unmatchedApplicationCount } from './applicationCountsModel';
import { AssumptionsNote } from './AssumptionsNote';
import { formatDateJst, formatDateTimeJst, formatPeriodJst, formatYen, orderCategories, plainWording } from './format';
import './ab-comparison.css';

const number = (value: number | null | undefined, unit = '') => value === null || value === undefined ? '未取得・算出不可' : `${value.toLocaleString('ja-JP', { maximumFractionDigits: 2 })}${unit}`;
const yen = (value: number | null | undefined) => formatYen(value, '未取得・算出不可');
const dimensions = [{ id: 'gender', label: '性別' }, { id: 'age', label: '年代' }, { id: 'prefecture', label: '都道府県' }, { id: 'municipality', label: '市区町村' }] as const;
function MetricValues({ row }: { row: HrhPerformanceRow | undefined }) {
  if (!row) return <p>課金・クリック実績は未接続、または期間未選択です。</p>;
  const ratios = performanceRatios(row);
  return <dl className="ab-values">{[
    ['表示数', number(row.impressions, '回')], ['クリック数', number(row.clicks, '回')], ['クリック率', number(ratios.ctr, '%')],
    ['費用', yen(row.cost_yen)], ['媒体応募数', number(row.applications, '件')], ['クリック単価', yen(ratios.cpc)], ['媒体応募単価', yen(ratios.cpa)],
  ].map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>;
}
function Variant({ label, job, version, scope, metric, onVersion, onMetric }: {
  label: string; job: JobCopyRecord; version: CopyVersion | undefined; scope: AbScope; metric: string;
  onVersion: (value: string) => void; onMetric: (value: string) => void;
}) {
  const row = job.hrhPerformance?.rows.find(item => item.period_start === metric);
  const reasons = job.applicantReasons;
  // Only application reasons: transfer reasons (why the person leaves a job) are a different question.
  const applicationItems = reasons?.available ? reasons.items.filter(item => APPLICATION_TEXT_SOURCES.includes(item.sourceProperty)) : [];
  const texts = applicationItems.filter(item => scope === 'record' || item.versionId === version?.id);
  return <section className="ab-variant" aria-label={`${label}求人の比較内容`}>
    <h3>{label}：{job.title}</h3><p>{job.company} · {job.location} · {job.media}</p>
    {job.hubspotUrl && <a href={job.hubspotUrl} target="_blank" rel="noreferrer">{label}のHubSpot求人を開く</a>}
    <label>{label}の本文の版<select value={version?.id ?? ''} onChange={event => { onVersion(event.target.value); }}><option value="">版を選択</option>{publishedVariants(job).map(item => <option key={item.id} value={item.id}>{item.label} · {formatDateJst(item.observedAt)}</option>)}</select></label>
    <p>応募数（{scope === 'record' ? '求人レコード全体' : '選んだ版に結びついた応募'}）：<strong>{number(variantCount(job, version, scope), '件')}</strong></p>
    <p>{scope === 'record' ? `応募集計取得日時：${formatDateTimeJst(job.overallApplications?.fetchedAt, '未取得')}` : `確定 ${number(version?.applications?.confirmed, '件')} / 推定 ${number(version?.applications?.estimated, '件')}`}</p>
    <p>求人全体で、どの版への応募か不明：{number(unmatchedApplicationCount(job), '件')}（選択版の応募数には含めません）</p>
    {version ? <><ImageGallery title={`${label}の掲載画像`} images={version.images ?? imagesByVersion[version.id]} />{version.historicalImageBytesAvailable === false && <p className="jc-notice">当時の画像原本は未保存です。</p>}<details><summary>{label}の求人本文を全文確認</summary><pre className="jc-body">{version.body}</pre>{version.note.trim() && <p>{plainWording(version.note)}</p>}</details></> : <p>掲載を確認できた本文は未取得です。確認待ちの文面・AI案は比較対象にしていません。</p>}
    <label>{label}の課金実績期間<select value={metric} onChange={event => { onMetric(event.target.value); }}><option value="">実績期間を選択</option>{job.hrhPerformance?.rows.map(item => <option key={item.period_start} value={item.period_start}>{formatPeriodJst(item.period_start, item.period_end)}</option>)}</select></label>
    <MetricValues row={row} />
    <details className="jc-no-print"><summary>{label}の応募理由（内部閲覧）</summary><p>表示対象：{texts.length}記述。複数出典を含み、応募人数・回答率とは異なります。{reasons?.truncated ? '取得上限による一部表示です。' : ''}</p>
      {!reasons?.available ? <p>応募理由は未取得です。</p> : <><p>どの版への理由か不明な記述：{applicationItems.filter(item => item.versionId === null).length}件。選択版の反応には割り当てません。</p>{texts.map(item => <details key={item.id}><summary>原記録を開く · {reasonSourceLabels[item.sourceProperty] ?? '記録欄'} · {item.applicationDate ? formatDateJst(item.applicationDate, item.applicationDate) : '応募日不明'}</summary><p>個人情報を含む可能性のある原記録です。対応する版：{item.versionId === null ? '不明' : job.versions.find(entry => entry.id === item.versionId)?.label ?? '取得した版'}</p><blockquote>{item.text}</blockquote></details>)}</>}
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
  return <section className="ab-comparison" aria-label="2求人のA/B比較"><h2>2つの求人をA/B比較する</h2><p>別の求人や文面でも、同じ募集の比較対象として組み合わせられます。元のレコード・履歴・応募はそれぞれ保持します。</p>
    <div className="ab-controls jc-no-print"><label>比較グループ名<input value={name} maxLength={120} placeholder="例：配送募集・画像の訴求比較" onChange={event => { setName(event.target.value); }} /></label><label>検証したい仮説<textarea value={hypothesis} maxLength={1000} rows={2} onChange={event => { setHypothesis(event.target.value); }} /></label>
      <label>Bとして比較する求人<select value={otherId} onChange={event => { chooseB(event.target.value); }}><option value="">別の求人を選択</option>{records.filter(item => item.id !== job.id).map(item => <option key={item.id} value={item.id}>{item.company} · {item.title} · {item.media}</option>)}</select></label>
      <label><input type="checkbox" checked={confirmed} disabled={!other} onChange={event => { setConfirmed(event.target.checked); }} />同じ募集として比較する組み合わせを確認した</label>
      <label>応募の比較範囲<select value={scope} onChange={event => { setScope(event.target.value === 'version' ? 'version' : 'record'); }}><option value="record">求人レコード全体（取得済み応募）</option><option value="version">選択した本文の版（確定・推定の対応分）</option></select></label>
      <p className="jc-muted">比較設定はこの画面の中だけに残ります。再読み込み・求人の切り替えで消え、共有の保存はまだできません。</p>
    </div>
    {!other ? <p role="status">B求人を選ぶと、A/Bの本文・画像・応募結果が並びます。候補は読み込み済み求人です。</p> : <>
      <h3>{name || '名称未設定の比較グループ'}</h3>{hypothesis && <p>仮説：{hypothesis}</p>}
      {!confirmed && <p className="jc-notice" role="status">組み合わせ未確認。比較対象の募集内容・取引先・勤務地を確認してください。</p>}
      {(job.company !== other.company || job.location !== other.location || job.media !== other.media) && <p className="jc-notice">取引先・勤務地・媒体に違いがあります。募集条件や露出条件の違いも含む比較です。</p>}
      <AssumptionsNote summary={scope === 'record' ? '応募者構成・理由は、取得済みの求人全体の値です（選択した文面や課金期間だけの値ではありません）。' : '応募者構成は、選択した版に対応する確定・推定の応募です。'} items={[
        scope === 'version' && 'どの版への応募か不明な応募は除き、理由も版が分かる記述だけを表示します。',
        '媒体の実績期間と本文の版の対応は未確認です。',
        '表示の無作為な振り分けは行っていません。同時の掲載も、時期をずらした掲載も並べて比べるだけで、どちらが良いかは自動で判定しません。',
      ]} />
      <div className="ab-pair"><Variant label="A" job={job} version={a} scope={scope} metric={metricA} onVersion={setVersionA} onMetric={setMetricA} /><Variant label="B" job={other} version={b} scope={scope} metric={metricB} onVersion={setVersionB} onMetric={setMetricB} /></div>
      <section aria-label="A/B実績期間の比較"><h3>課金実績期間の比較</h3>{periods ? <><p>A：{formatPeriodJst(rowA?.period_start, rowA?.period_end)}（{periods.daysA}日） / B：{formatPeriodJst(rowB?.period_start, rowB?.period_end)}（{periods.daysB}日） · 重なる日数：{periods.overlapDays}日</p><p>クリック率の差（B−A）：{number(periods.ctrDeltaPp, 'ポイント')} / クリック→媒体応募率：A {number(periods.cvrA, '%')}・B {number(periods.cvrB, '%')}</p><p>期間の集計値を日割りで補間しません。露出配分・予算・曜日・市場環境も確認してください。</p></> : <p>両求人の実績期間を選ぶと比較できます。HubSpot応募数を媒体の転換率計算には使いません。</p>}</section>
      <section aria-label="A/B応募者構成の比較"><h3>応募者構成（件数・割合）</h3>{dimensions.map(dimension => {
        const compared = compareDistributions(variantDistribution(job, a, scope, dimension.id), variantDistribution(other, b, scope, dimension.id), { areas: dimension.id === 'prefecture' || dimension.id === 'municipality' });
        const rows = compared === null ? null : orderCategories(dimension.id, compared);
        return <details key={dimension.id}><summary>{dimension.label}のA/B比較</summary>{rows === null ? <p>片方または両方の属性が未取得です。</p> : <div className="ab-table-scroll" tabIndex={0} role="region" aria-label={`${dimension.label}比較表の横スクロール`}><table><caption>{dimension.label} · {scope === 'record' ? '求人全体' : '選択版対応'} · 割合の分母は属性不明を含む応募</caption><thead><tr><th>区分</th><th>A件数</th><th>A割合</th><th>B件数</th><th>B割合</th><th>B−A</th></tr></thead><tbody>{rows.map(row => <tr key={row.category}><th scope="row">{row.category}</th><td>{row.beforeCount}</td><td>{number(row.beforePercentage, '%')}</td><td>{row.afterCount}</td><td>{number(row.afterPercentage, '%')}</td><td>{number(row.deltaPp, 'ポイント')}</td></tr>)}</tbody></table>{!rows.length && <p>両方の対象応募は0件です。割合は算出できません。</p>}</div>}</details>;
      })}</section>
    </>}
  </section>;
}
