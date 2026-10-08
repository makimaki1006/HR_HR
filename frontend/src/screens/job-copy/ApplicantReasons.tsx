import { useState } from 'react';
import type { CopyVersion, JobCopyRecord } from './data';
import { TEXT_SOURCES, reasonCohorts, reasonSourceLabels } from './applicantReasonsModel';
import { ReasonCategorySummary } from './ReasonCategorySummary';
import type { ApplicantReason } from './applicantReasonsModel';
import { AssumptionsNote } from './AssumptionsNote';
import { formatDateJst, formatDateTimeJst } from './format';
import { maskPersonalDetails } from './personalText';
import './applicant-reasons.css';

function ReasonTexts({ items }: { items: ApplicantReason[] }) {
  return <ol className="ar-texts">{items.map(item => <li key={item.id}><p className="ar-source">出典: {reasonSourceLabels[item.sourceProperty] ?? 'HubSpotの記録欄'}</p><p>応募日: {item.applicationDate ? formatDateJst(item.applicationDate, item.applicationDate) : '不明'} · 記述を集めた日時: {formatDateTimeJst(item.collectedAt, item.collectedAt ?? '不明')}</p><details><summary>記録された文を開く（社内確認用）</summary><p>市区町村より細かい住所（町名・番地・建物名と部屋番号）・電話番号・メールアドレス・「さん」「様」の付いた名前は、読み取れた範囲で「＊＊」に置き換えています。読み取れない書き方の住所や、それ以外の個人情報が残っていることがあります。顧客向けの印刷には含めません。</p><blockquote>{maskPersonalDetails(item.text)}</blockquote></details></li>)}</ol>;
}

export function ApplicantReasons({ job, before, after }: { job: JobCopyRecord; before?: CopyVersion | undefined; after?: CopyVersion | undefined }) {
  const [source, setSource] = useState('all');
  const collection = job.applicantReasons;
  const cohorts = reasonCohorts(collection, before?.id, after?.id, source);
  return <section className="ar-reasons" aria-label="応募理由の記述比較"><h2>応募理由・志望動機の記述</h2>
    <AssumptionsNote summary="求人文面の版と、HubSpotに記録された応募理由を並べて確認します。" items={['応募者の気持ちを推測する評価ではありません。', 'ヒアリングの記録も含むため、すべてが応募者本人の言葉とは限りません。', '件数は複数の記録欄の記述の数で、応募人数や回答率とは異なります。']} />
    {!collection?.available || !cohorts ? <p className="ar-unavailable" role="status">応募理由の自由記述は未取得です。理由がない応募や0件とは判定していません。</p> : <>
      <p>取得対象: 応募{collection.totalApplicants}件・記録された理由{collection.totalSourceValues}件 · 未記録{collection.missing}件 · 空欄{collection.blank}件</p>
      <p>理由データの取得日時: {formatDateTimeJst(collection.fetchedAt, collection.fetchedAt ?? '不明')}</p>
      <ReasonCategorySummary collection={collection} />
      <h3>版ごとの記述</h3>
      {collection.truncated && <p className="jc-notice">表示対象は取得上限による一部です。表示件数を全記述件数として扱いません。</p>}
      <label className="ar-source-filter jc-no-print">理由の出典<select aria-label="理由の出典" value={source} onChange={event => { setSource(event.target.value); }}><option value="all">すべての出典</option>{Object.keys(collection.sourceCounts).filter(property => TEXT_SOURCES.includes(property)).map(property => <option key={property} value={property}>{reasonSourceLabels[property] ?? 'HubSpotの記録欄'}</option>)}</select></label>
      {before?.id === after?.id && before && <p>同じ版を選んでいます。</p>}
      {!cohorts.versionAttributionAvailable && <p className="jc-notice">理由と版の対応は未取得です。変更前後には割り当てず、「どの版への理由か不明な記述」に表示します。</p>}
      <div className="ar-pair">{[{ title: '比較元の記述', version: before, rows: cohorts.before }, { title: '比較先の記述', version: after, rows: cohorts.after }].map(group => <section key={group.title} aria-label={group.title}><h3>{group.title}</h3><p>{group.version?.label ?? '版なし'}</p>{cohorts.versionAttributionAvailable ? <><p>表示対象{group.rows.length}件</p>{group.rows.length ? <ReasonTexts items={group.rows} /> : <p>選択版に対応する表示対象の記述はありません。</p>}</> : <p>版との対応は未取得です。</p>}</section>)}</div>
      <section aria-label="どの版への理由か不明な記述"><h3>どの版への理由か不明な記述 · 表示対象{cohorts.unknown.length}件</h3><p>応募日があっても、理由を取得した日時から掲載された版を推定していません。変更前後の反応とは判断できません。</p>{cohorts.unknown.length ? <ReasonTexts items={cohorts.unknown} /> : <p>該当する記述はありません。</p>}</section>
      {cohorts.other.length > 0 && <p>選択した2版以外の表示対象記述: {cohorts.other.length}件。前後比較には含めません。</p>}
    </>}
  </section>;
}
