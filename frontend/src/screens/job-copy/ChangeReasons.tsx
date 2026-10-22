import { useMemo, useState } from 'react';
import type { JobCopyRecord } from './data';
import { createdDateNote } from './liveApplications';
import { changeDateLabel, changeReasonBlocks, changeReasonSummary } from './changeReasonsModel';
import type { ChangeReasonBlock, ReasonWeeks, ReasonWindow } from './changeReasonsModel';
import { formatDateJst } from './format';
import { md } from './listingEventsModel';
import { maskPersonalDetails } from './personalText';
import { MASK_NOTE, REASON_CATEGORIES } from './reasonCategories';
import { persistableListingId, useSavedNote } from './savedNotes';
import { addDays, asOfDate } from './timelineModel';
import './change-reasons.css';

const WEEKS: readonly ReasonWeeks[] = [2, 4, 8];
const EXCERPT = 120;

function rangeText(window: ReasonWindow): string {
  return window.days > 0 ? `${md(window.start)}〜${md(addDays(window.end, -1))}（${String(window.days)}日間）` : 'この本文が出ていたと確かな日がないため、数えていません';
}
function limitText(window: ReasonWindow): string | null {
  const shortened = window.shortened;
  if (!shortened) return null;
  const date = shortened.date ? `（${md(shortened.date)}）` : '';
  if (shortened.reason === 'previous_change') return `前の変更${date}までの${String(window.days)}日で区切っています`;
  if (shortened.reason === 'next_change') return `次の変更${date}までの${String(window.days)}日で区切っています`;
  if (shortened.reason === 'listing_start') return `掲載の記録の始まり${date}からの${String(window.days)}日で区切っています`;
  return `${date.replace(/^（|）$/g, '')}までの${String(window.days)}日で区切っています（その先はまだ確認できていません）`;
}

function WindowColumn({ title, window }: { title: string; window: ReasonWindow }) {
  const reasons = window.reasons;
  const limit = window.days > 0 ? limitText(window) : null;
  return <div className="cr-column" role="group" aria-label={title}>
    <h4>{title}</h4>
    <p className="cr-range">{rangeText(window)}</p>
    {limit && <p className="cr-limit">{limit}</p>}
    {window.days === 0 ? null : <>
    <p className="cr-count">応募 {window.applications === null ? '未取得' : `${String(window.applications)}件`}</p>
    {reasons === null ? <p>応募理由は未取得です。</p> : <>
      <p className="cr-count">応募理由の記録 {String(reasons.n)}件</p>
      {reasons.n === 0 ? <p className="cr-empty">この期間に応募理由の記録はありません</p> : <>
        <table className="ar-table cr-table"><thead><tr><th scope="col">区分</th><th scope="col">件数</th><th scope="col">内訳</th></tr></thead>
          <tbody>{REASON_CATEGORIES.map(category => {
            const row = reasons.counts.find(item => item.category === category);
            return <tr key={category}><th scope="row">{category}</th><td>{String(row?.total ?? 0)}件</td><td>{row && row.total > 0 ? [row.selected ? `選択済み${String(row.selected)}` : '', row.estimated ? `推定${String(row.estimated)}` : ''].filter(Boolean).join('・') : ''}</td></tr>;
          })}</tbody></table>
        {reasons.unclassified > 0 && <p className="ar-note">分類できない記録 {String(reasons.unclassified)}件</p>}
      </>}
    </>}
    {window.texts.length > 0 && <details className="cr-texts"><summary>応募者が書いた文を開く（社内確認用）</summary>
      <p className="ar-note">{MASK_NOTE}顧客向けの印刷には含めません。</p>
      <ol className="ar-texts">{window.texts.map(item => <li key={item.id}><p className="ar-source">出典: {item.sourceLabel}</p><p>応募日: {item.applicationDate ? formatDateJst(item.applicationDate, item.applicationDate) : '不明'}</p><blockquote>{maskPersonalDetails(item.text)}</blockquote></li>)}</ol>
    </details>}
  </>}</div>;
}

function Block({ block }: { block: ChangeReasonBlock }) {
  const label = `${changeDateLabel(block.window)} ${block.kinds.join('・')}の変更`;
  return <section className="cr-block" aria-label={label} data-testid="change-reason-block">
    <h3>{changeDateLabel(block.window)} · {block.kinds.join('・')}</h3>
    {block.summary && <p className="cr-summary">{block.summary}</p>}
    {!block.window.exact && <p className="ar-note">この変更は、文面を確認した 2 つの日の間のどこかで起きました。その間の日は、前後どちらの応募にも数えていません。</p>}
    <div className="cr-pair"><WindowColumn title={`変更前 ${String(block.before.days)}日`} window={block.before} /><WindowColumn title={`変更後 ${String(block.after.days)}日`} window={block.after} /></div>
  </section>;
}

export function ChangeReasons({ job, onOpenFactors }: { job: JobCopyRecord; onOpenFactors: () => void }) {
  const [weeks, setWeeks] = useState<ReasonWeeks>(4);
  const asOf = asOfDate(job);
  const blocks = useMemo(() => changeReasonBlocks(job, weeks, asOf), [job, weeks, asOf]);
  const summary = useMemo(() => changeReasonSummary(job), [job]);
  const createdNote = createdDateNote(job.overallApplications?.createdDateCount);
  const note = useSavedNote(persistableListingId(job), 'factor');
  const memo = note.status === 'ready' && note.value?.trim() ? note.value.trim() : null;
  return <section className="cr-changes" aria-label="変更ごとの応募理由">
    <p className="cr-lead">応募理由の記録はまだ少なく、1 つの求人・期間の件数だけで変更の効果は判断できません。</p>
    <ul className="cr-notes">
      <li>HubSpot に記録された分だけを数えています。</li>
      {createdNote && <li>{createdNote}。</li>}
      {summary.undated > 0 && <li>応募日も作成日も分からない記録 {String(summary.undated)}件は、どの期間にも入れていません。</li>}
      {summary.multiListing ? <li>ほかの求人にも関連する応募 {String(summary.multiListing)}件は数えていません。</li> : null}
    </ul>
    <div className="cr-weeks jc-no-print" role="group" aria-label="前後の長さ">
      {WEEKS.map(value => <button key={value} type="button" className="jc-button" aria-pressed={weeks === value} aria-label={`前後 ${String(value)}週`} onClick={() => { setWeeks(value); }}>{value}週</button>)}
    </div>
    <div className="cr-memo" aria-label="要因メモ">
      {memo ? <p>要因メモ: {memo.length > EXCERPT ? `${memo.slice(0, EXCERPT)}…` : memo}</p> : <p>この求人の要因メモはまだありません</p>}
      <button type="button" className="jc-button" onClick={onOpenFactors}>要因メモを開く</button>
    </div>
    {blocks.length === 0 ? <p className="cr-empty">この求人で給与・本文・画像の変更は見つかっていません</p> : blocks.map(block => <Block key={block.versionId} block={block} />)}
  </section>;
}
