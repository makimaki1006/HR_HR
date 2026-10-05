import { useState } from 'react';
import type { CopyVersion, JobCopyRecord } from './data';
import { reasonCohorts, reasonSourceLabels } from './applicantReasonsModel';
import type { ApplicantReason } from './applicantReasonsModel';
import './applicant-reasons.css';

function ReasonTexts({ items }: { items: ApplicantReason[] }) {
  return <ol className="ar-texts">{items.map(item => <li key={item.id}><p className="ar-source">出典: {reasonSourceLabels[item.sourceProperty] ?? item.sourceProperty}（{item.sourceProperty}）</p><p>応募日: {item.applicationDate ?? '不明'} · 記述収集日時: {item.collectedAt ?? '不明'}</p><details><summary>内部閲覧用の原記録を開く</summary><p>個人情報を含む可能性のある原文です。匿名化された内容ではありません。既定の顧客向け印刷には含めません。</p><blockquote>{item.text}</blockquote></details></li>)}</ol>;
}

export function ApplicantReasons({ job, before, after }: { job: JobCopyRecord; before?: CopyVersion | undefined; after?: CopyVersion | undefined }) {
  const [source, setSource] = useState('all');
  const collection = job.applicantReasons;
  const cohorts = reasonCohorts(collection, before?.id, after?.id, source);
  return <section className="ar-reasons" aria-label="応募理由の記述比較"><h2>応募理由・志望動機の記述</h2>
    <p>求人文面の観測版と、記録された理由を並べて確認します。文面変更による効果や、応募者の気持ちを推測する評価ではありません。ヒアリング記録も含むため、すべてを応募者本人の言葉とは断定しません。</p>
    {!collection?.available || !cohorts ? <p className="ar-unavailable" role="status">応募理由の自由記述は未取得です。理由がない応募や0件とは判定していません。</p> : <>
      <p>取得対象: 応募{collection.totalApplicants}件・出典プロパティ観測{collection.totalSourceValues}件 · 未記録値{collection.missing}件 · 空欄{collection.blank}件</p>
      <p>理由データ取得日時: {collection.fetchedAt ?? '不明'} · 複数出典の記述件数です。応募人数・回答率とは異なります。</p>
      {collection.truncated && <p className="jc-notice">表示対象は取得上限による一部です。表示件数を全記述件数として扱いません。</p>}
      <label className="ar-source-filter jc-no-print">理由の出典<select aria-label="理由の出典" value={source} onChange={event => { setSource(event.target.value); }}><option value="all">すべての出典</option>{Object.keys(collection.sourceCounts).map(property => <option key={property} value={property}>{reasonSourceLabels[property] ?? property}</option>)}</select></label>
      {before?.id === after?.id && before && <p>同じ観測版を選択しています。</p>}
      {!cohorts.versionAttributionAvailable && <p className="jc-notice">理由と掲載観測版の対応は未取得です。変更前後には割り当てず、版対応不明の欄に表示します。</p>}
      <div className="ar-pair">{[{ title: '比較元の記述', version: before, rows: cohorts.before }, { title: '比較先の記述', version: after, rows: cohorts.after }].map(group => <section key={group.title} aria-label={group.title}><h3>{group.title}</h3><p>{group.version?.label ?? '観測版なし'}</p>{cohorts.versionAttributionAvailable ? <><p>表示対象{group.rows.length}件</p>{group.rows.length ? <ReasonTexts items={group.rows} /> : <p>選択版に対応する表示対象の記述はありません。</p>}</> : <p>版との対応は未取得です。</p>}</section>)}</div>
      <section aria-label="版対応不明の記述"><h3>版対応不明の記述 · 表示対象{cohorts.unknown.length}件</h3><p>応募日があっても、理由を取得した日時から掲載版を推定していません。変更前後の反応とは判断できません。</p>{cohorts.unknown.length ? <ReasonTexts items={cohorts.unknown} /> : <p>表示対象の版対応不明記述はありません。</p>}</section>
      {cohorts.other.length > 0 && <p>選択した2版以外の表示対象記述: {cohorts.other.length}件。前後比較には含めません。</p>}
    </>}
  </section>;
}
