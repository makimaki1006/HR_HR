import { useState } from 'react';
import type { CopyVersion, JobCopyRecord } from './data';
import { compareImages, referenceImages, imagesByVersion } from './images';
import { ImageGallery } from './ImageGallery';
import { ApplicantReasons } from './ApplicantReasons';
import { compareDistributions, compositionDistribution, displayDistribution } from './applicantCompositionModel';
import { jobApplicationTotal, noLinkedApplicationsMessage, unmatchedApplicationCount } from './applicationCountsModel';
import type { ApplicantDimension } from './applicantCompositionModel';
import { AssumptionsNote } from './AssumptionsNote';
import { InfoTip } from './InfoTip';
import { formatDateJst, formatDateTimeJst, orderCategories } from './format';
import './applicant-composition.css';

const dimensions: { id: ApplicantDimension; label: string }[] = [{ id: 'gender', label: '性別' }, { id: 'age', label: '年代' }, { id: 'prefecture', label: '都道府県' }, { id: 'municipality', label: '市区町村' }];
const percent = (value: number | null) => value === null ? '算出不可' : `${value.toFixed(1)}%`;
const delta = (value: number | null) => value === null ? '算出不可' : `${value > 0 ? '+' : ''}${value.toFixed(1)}pt`;
const fullDate = (value: string) => formatDateTimeJst(value, '日時不明');

function Period({ version, label }: { version: CopyVersion | undefined; label: string }) {
  if (version?.distributions) return <section className="ac-period"><h3>{label}: {version.label}</h3><div className="ac-period-line"><InfoTip className="jc-infotip-left" label="応募集計対象日"><p>応募日と、その日に取得できた版を日付単位で突き合わせています。取得できなかった日は含めません。</p></InfoTip>: {version.observationDates === undefined ? '対象日の一覧は未取得' : version.observationDates.length ? version.observationDates.map(day => formatDateJst(day, day)).join('、') : '該当する取得日なし'}</div><p>属性: 現在取得できる値 · 取得日時 {version.attributesFetchedAt ? fullDate(version.attributesFetchedAt) : '未取得'}</p></section>;
  return <section className="ac-period"><h3>{label}: {version?.label ?? '版なし'}</h3><p>掲載期間: {version?.publishedFrom ? fullDate(version.publishedFrom) : '開始未取得'} → {version?.publishedUntil ? fullDate(version.publishedUntil) : '終了未確認'}</p><p>本文の取得: {version ? fullDate(version.observedAt) : '未取得'} · {version?.certainty === 'confirmed' ? '期間確定' : version?.certainty === 'estimated' ? '期間推定' : '期間不明'}</p><p>属性取得日時: 未取得</p></section>;
}

export function ApplicantComposition({ job, selection, onSelectionChange, includeReasons = true }: { job: JobCopyRecord; selection?: [string, string]; onSelectionChange?: (selection: [string, string]) => void; includeReasons?: boolean }) {
  const versions = job.versions.filter(version => version.kind === 'published');
  const [beforeId, setBeforeId] = useState(versions[0]?.id ?? '');
  const [afterId, setAfterId] = useState(versions[1]?.id ?? versions[0]?.id ?? '');
  const before = versions.find(version => version.id === (selection?.[0] ?? beforeId)) ?? versions[0];
  const after = versions.find(version => version.id === (selection?.[1] ?? afterId)) ?? versions.at(-1);
  const beforeDistribution = compositionDistribution(job, before, 'gender');
  const afterDistribution = compositionDistribution(job, after, 'gender');
  const images = (version: CopyVersion | undefined) => version ? version.images ?? imagesByVersion[version.id] : undefined;
  const imageComparison = compareImages(referenceImages(before), referenceImages(after));

  return <section className="ac-composition" aria-label="版別の応募者構成">
    {job.dataSource === 'hubspot' && job.overallApplications && <section className="ac-overall" aria-label="求人全体の実応募者構成"><h2>求人全体の実応募者構成</h2><p>応募{job.overallApplications.total}件 · 応募日不明{job.overallApplications.missingDate}件 · どの版への応募か不明{unmatchedApplicationCount(job) ?? job.overallApplications.total}件</p>
      {dimensions.map(dimension => {
        const distribution = displayDistribution(job.overallApplications?.distributions[dimension.id], dimension.id);
        return <section key={dimension.id} className="ac-chart" aria-label={`求人全体の${dimension.label}`}><h3>{dimension.label}</h3>{job.overallApplications?.total === 0 ? <p>求人全体の応募は0件です。割合は算出できません。</p> : !distribution ? <p>この属性は未取得です。0件・0%とは判定していません。</p> : <div className="ac-chart-rows">{orderCategories(dimension.id, distribution.categories).map(row => <div key={row.category} className="ac-chart-row"><strong>{row.category}</strong><div className="ac-bars" aria-hidden="true"><div className="ac-track"><span className="ac-before" style={{ width: `${String(row.percentage ?? 0)}%` }} /></div></div><span>{row.count}件 ({percent(row.percentage)})</span></div>)}</div>}</section>;
      })}
      <AssumptionsNote className="ac-caveat" summary={`HubSpotに記録された求人全体の応募の構成です（属性は ${fullDate(job.overallApplications.fetchedAt)} 時点の値）。`} items={['どの版への応募か不明な応募も含むため、下の版ごとの比較とは分母が異なります。', 'どの版への応募か不明な応募は、各版に割り当てていません。', '地域は都道府県と市区町村までに丸め、3人未満の地域は「その他」にまとめています。']} />
    </section>}
    <header><h2>文面・画像と応募者構成を比べる</h2><p>掲載を確認できた版どうしの比較です。確認待ちの文面・未掲載のAI案は比較対象に含めません。</p></header>
    <div className="ac-selectors"><label>構成比較元<select value={before?.id ?? ''} onChange={event => { setBeforeId(event.target.value); onSelectionChange?.([event.target.value, after?.id ?? '']); }}>{!versions.length && <option value="">掲載版なし</option>}{versions.map(version => <option key={version.id} value={version.id}>{version.label}</option>)}</select></label><label>構成比較先<select value={after?.id ?? ''} onChange={event => { setAfterId(event.target.value); onSelectionChange?.([before?.id ?? '', event.target.value]); }}>{!versions.length && <option value="">掲載版なし</option>}{versions.map(version => <option key={version.id} value={version.id}>{version.label}</option>)}</select></label></div>
    <div className="ac-periods"><Period version={before} label="比較元" /><Period version={after} label="比較先" /></div>
    {(before?.historicalImageBytesAvailable === false || after?.historicalImageBytesAvailable === false) && <p className="ac-caveat">過去時点の画像原本は未保存です。後日取得した表示画像があっても、当時の画像内容とは確認できません。</p>}
    <details className="ac-image-details"><summary>掲載画像を比較・拡大</summary><div className="ac-image-pair"><ImageGallery title="構成比較元の掲載画像" images={images(before)} /><ImageGallery title="構成比較先の掲載画像" images={images(after)} /></div></details>
    <p>画像比較: {imageComparison.status === 'unknown' ? '未取得の画像があり判定不能' : imageComparison.status === 'same_reference' ? '参照・並び順は同じです。画像内容の一致は未検証です。' : `追加${String(imageComparison.added.length)}点・削除${String(imageComparison.removed.length)}点${imageComparison.reordered ? '・並び順変更あり' : ''}`}</p>
    <AssumptionsNote className="ac-caveat" summary={`構成の差は、2つの版を並べて見るための数字です。${unmatchedApplicationCount(job) !== null ? `どの版への応募か不明: ${String(unmatchedApplicationCount(job))}件（下の版ごとのグラフには含めません）。` : ''}`} items={[
      '本文・画像・掲載期間が同時に変わる場合があります。構成の差だけでは、文面や画像の変更が原因かどうかは分かりません。',
      job.hrhPerformance ? '媒体の期間別実績は「課金・クリック」で確認できます。本文の版との対応は未確認です。' : '課金情報は未取得です。後日、掲載期間と費用・応募単価を合わせて確認します。',
      unmatchedApplicationCount(job) !== null && '「どの版への応募か不明」には、応募日が無いもの、その日の版を取得できなかったもの、紐づけがはっきりしないものを含みます。',
    ]} />
    {includeReasons && <ApplicantReasons job={job} before={before} after={after} />}
    {beforeDistribution === null || afterDistribution === null ? <p className="ac-unavailable" role="status">応募者の属性データは未取得です。取得した媒体の求人にも架空の応募者を割り当てません。0件・0%とは判定していません。</p> : <>
      <p className="ac-demo">{job.dataSource === 'hubspot' ? 'HubSpotの応募を、変更に気づいた日の版に応募日で割り当てた件数です。属性は現在取得できる値です。' : '架空の応募者属性による操作デモです。'}比較元{String(beforeDistribution.total)}件・比較先{String(afterDistribution.total)}件。分母には属性不明も含めます。{job.dataSource !== 'hubspot' && '各版に応募日で結びついた架空の応募だけを含めます。'}</p>
      {dimensions.map(dimension => {
        const compared = compareDistributions(compositionDistribution(job, before, dimension.id), compositionDistribution(job, after, dimension.id), { areas: dimension.id === 'prefecture' || dimension.id === 'municipality' });
        const comparison = compared === null ? null : orderCategories(dimension.id, compared);
        if (comparison === null) return <section key={dimension.id} className="ac-chart"><h3>{dimension.label}</h3><p>この属性は未取得です。0件・0%とは判定していません。</p></section>;
        if (comparison.length === 0) return <section key={dimension.id} className="ac-chart"><h3>{dimension.label}</h3><p>{noLinkedApplicationsMessage(jobApplicationTotal(job))}。割合は算出できません。</p></section>;
        return <section key={dimension.id} className="ac-chart" aria-label={`${dimension.label}の構成比較`}><h3>{dimension.label}</h3><p className="ac-legend"><span>比較元</span><span>比較先</span> · 両方とも同じ目盛りの割合</p>
          <div className="ac-chart-rows">{comparison.map(item => <div key={item.category} className="ac-chart-row"><strong>{item.category}</strong><div className="ac-bars" aria-hidden="true"><div className="ac-track"><span className="ac-before" style={{ width: `${String(item.beforePercentage ?? 0)}%` }} /></div><div className="ac-track"><span className="ac-after" style={{ width: `${String(item.afterPercentage ?? 0)}%` }} /></div></div><span>{String(item.beforeCount)}件 ({percent(item.beforePercentage)}) → {String(item.afterCount)}件 ({percent(item.afterPercentage)})<br /><b>{delta(item.deltaPp)}</b></span></div>)}</div>
          <details><summary>{dimension.label}の数値表を開く</summary><table><caption>{dimension.label}の応募件数・割合・割合差</caption><thead><tr><th scope="col">区分</th><th scope="col">比較元</th><th scope="col">比較先</th><th scope="col">割合差</th></tr></thead><tbody>{comparison.map(item => <tr key={item.category}><th scope="row">{item.category}</th><td>{String(item.beforeCount)}件 / {percent(item.beforePercentage)}</td><td>{String(item.afterCount)}件 / {percent(item.afterPercentage)}</td><td>{delta(item.deltaPp)}</td></tr>)}</tbody></table></details>
        </section>;
      })}
    </>}
  </section>;
}
