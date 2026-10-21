import { useEffect, useRef } from 'react';
import type { JobCopyRecord } from './data';
import { compareCopy } from './diff';
import { compareImages, referenceImages, imagesByVersion } from './images';
import { ApplicantComposition } from './ApplicantComposition';
import { compareDistributions, compositionDistribution } from './applicantCompositionModel';
import { observationWindowError, reportBillingText } from './consultantReviewModel';
import { AssumptionsNote } from './AssumptionsNote';
import { SavedNoteBar } from './SavedNoteBar';
import { persistableListingId, useSavedNote } from './savedNotes';
import type { BillingEntry } from './timelineModel';
import './consultant-review.css';

const REVIEW_KEYS = ['hypothesis', 'criterion', 'findings', 'action', 'start', 'end'] as const;
/** 保存する形。進行状況・変更対象と、記入した欄だけ。比較する本文の選択は保存しない。 */
function serializeReview(draft: ConsultantDraft): string {
  const fields: Record<string, string> = {};
  for (const key of REVIEW_KEYS) if ((draft.fields[key] ?? '') !== '') fields[key] = draft.fields[key] ?? '';
  return JSON.stringify({ stage: draft.stage, target: draft.target, fields });
}
function parseReview(text: string): Pick<ConsultantDraft, 'stage' | 'target' | 'fields'> | null {
  try {
    const raw = JSON.parse(text) as { stage?: unknown; target?: unknown; fields?: unknown };
    if (typeof raw.stage !== 'string' || typeof raw.target !== 'string' || typeof raw.fields !== 'object' || raw.fields === null) return null;
    const fields: Record<string, string> = {};
    for (const key of REVIEW_KEYS) { const value = (raw.fields as Record<string, unknown>)[key]; if (typeof value === 'string') fields[key] = value; }
    return { stage: raw.stage, target: raw.target, fields };
  } catch { return null; }
}
const reviewIsBlank = (draft: ConsultantDraft) => draft.stage === 'plan' && draft.target === 'both' && REVIEW_KEYS.every(key => (draft.fields[key] ?? '') === '');

export interface ConsultantDraft { stage: string; target: string; fields: Record<string, string>; selection: [string, string] }
export function ConsultantReview({ job, draft, onDraft, billing }: { job: JobCopyRecord; draft: ConsultantDraft; onDraft: (value: ConsultantDraft) => void; billing?: readonly BillingEntry[] | undefined }) {
  const { stage, target, fields, selection } = draft;
  const note = useSavedNote(persistableListingId(job), 'review');
  const applied = useRef(false);
  // 保存済みの記録は、まだ何も入力していないときだけ入力欄に読み込む (入力中の内容は上書きしない)。
  useEffect(() => {
    if (applied.current || note.status !== 'ready') return;
    applied.current = true;
    const saved = note.value === null ? null : parseReview(note.value);
    if (saved && reviewIsBlank(draft)) onDraft({ ...draft, ...saved });
  }, [note.status, note.value, draft, onDraft]);
  const savedReview = note.value === null ? null : parseReview(note.value);
  const dirty = savedReview === null ? !reviewIsBlank(draft) : serializeReview(draft) !== serializeReview({ ...draft, ...savedReview });
  const persisted = note.status !== 'off';
  const periodError = observationWindowError(fields.start ?? '', fields.end ?? '');
  const setFields = (update: (value: Record<string, string>) => Record<string, string>) => { onDraft({ ...draft, fields: update(fields) }); };
  const versions = job.versions.filter(version => version.kind === 'published');
  const current = versions.find(version => version.id === selection[1]) ?? versions.at(-1);
  const previous = versions.find(version => version.id === selection[0]) ?? versions[0];
  const beforeRows = compositionDistribution(job, previous, 'gender');
  const afterRows = compositionDistribution(job, current, 'gender');
  const genderDeltas = compareDistributions(beforeRows, afterRows);
  const genderSummary = genderDeltas?.filter(item => (item.category === '男性' || item.category === '女性') && item.deltaPp !== null).map(item => `${item.category} ${item.deltaPp !== null && item.deltaPp > 0 ? '+' : ''}${item.deltaPp?.toFixed(1) ?? '—'}pt`).join(' / ');
  const change = compareCopy(versions.length > 1 ? previous?.body ?? null : null, current?.body ?? null);
  const images = current ? current.images ?? imagesByVersion[current.id] : undefined;
  const imageChange = compareImages(versions.length > 1 ? referenceImages(previous) : undefined, referenceImages(current));
  const demo = job.id.startsWith('demo-job-');
  const cost = reportBillingText(job, billing);
  const labels = { initial: '比較する過去の本文がないため、変更の有無は不明', changed: '本文の内容変更あり', unchanged: '本文は同じ', unavailable: '比較する本文が未取得のため、変更の有無は不明' };
  const questions: [string, string, string][] = [
    ['hypothesis', '仮説', '何を変えると、どの応募者層に届くと考えるか'],
    ['criterion', '評価基準', '対象層の応募件数・構成比など、期間とともに記入'],
    ['findings', '結果と根拠', '対象の本文・期間・集計値・取得できなかった情報'],
    ['action', '次に試すこと', '継続・変更・追加で確認することと、担当・期限'],
  ];
  return <section className="jc-consultant" aria-label="顧客報告と検証記録">
    <div className="jc-report-heading"><div><h2>顧客報告・次の施策</h2><p>{demo ? '架空の操作デモ' : job.dataSource === 'hubspot' ? `HubSpot取引先: ${job.company}` : '媒体CSVから取り込んだ求人・取引先は未連携'} · {job.title} · {job.media}</p></div><button className="jc-button jc-no-print" disabled={periodError !== null} onClick={() => { window.print(); }}>報告を印刷</button></div>
    <div className="jc-report-summary">
      <section><h3>何を変えたか</h3><strong>{labels[change.status]}</strong><p className="jc-report-image-change">画像参照：{imageChange.status === 'unknown' ? '比較資料なし・未判定' : imageChange.status === 'same_reference' ? '同じ参照・内容は未検証' : `追加${String(imageChange.added.length)}点・削除${String(imageChange.removed.length)}点${imageChange.reordered ? '・順序変更' : ''}`}</p><p>掲載画像：{images === undefined ? '未取得' : `${String(images.length)}点確認`}</p><p>{versions.length < 2 ? '過去の本文がなく、変更の有無は未判定です。' : '本文と画像の差分比較で変更箇所を確認してください。'}</p></section>
      <section><h3>比較期間の応募</h3><strong>{beforeRows !== null && afterRows !== null ? `${String(beforeRows.total)}件 → ${String(afterRows.total)}件` : '応募実績は未取得'}</strong><p>{previous?.label ?? '比較元なし'} → {current?.label ?? '比較先なし'}</p>{genderSummary && <p className="jc-report-gender">{genderSummary}（構成比差）</p>}<p>{demo ? '架空値です。下の比較する本文の選択に連動します。' : '応募と掲載された本文を紐付けてから集計します。'}</p></section>
      <section><h3>費用と応募単価</h3><strong>{cost.heading}</strong><p>{cost.detail}</p></section>
    </div>
    <AssumptionsNote className="jc-notice" summary="文面を確認した日時は、掲載が変わった正確な日時ではありません。" items={['毎日の確認では、前回の確認から今回の確認までの間を、変更があった期間として扱います。', '本文・画像・課金を同時に変えた場合、前後の差だけでは要因を特定できません。']} />
    <a className="jc-text-button jc-no-print" href="#job-copy-review-cycle">検証記録の入力へ移動 →</a>
    <ApplicantComposition job={job} billing={billing} selection={selection} onSelectionChange={value => { onDraft({ ...draft, selection: value }); }} />
    <section className="jc-cycle" id="job-copy-review-cycle"><h3>検証記録</h3><p>仮説 → 実施 → 結果の確認 → 次の施策を残すための記録欄です。{persisted ? '「保存」を押すと、この求人の記録として残り、案件管理を開ける人に表示されます。' : '入力はこの画面の中だけに残り、求人の切り替え・再読み込みで消えます。'}HubSpotには保存しません。</p>
      <div className="jc-cycle-controls"><label>進行状況<select value={stage} onChange={event => { onDraft({ ...draft, stage: event.target.value }); }}><option value="plan">Plan · 仮説と計画</option><option value="do">Do · 掲載変更を実施</option><option value="check">Check · 結果を確認</option><option value="act">Act · 次の施策を決定</option></select></label><label>変更対象<select value={target} onChange={event => { onDraft({ ...draft, target: event.target.value }); }}><option value="body">本文</option><option value="image">画像</option><option value="both">本文と画像</option></select></label>
      <label>確認開始日<input type="date" value={fields.start ?? ''} onChange={event => { setFields(value => ({ ...value, start: event.target.value })); }} /></label><label>確認終了日<input type="date" min={fields.start === '' ? undefined : fields.start} value={fields.end ?? ''} onChange={event => { setFields(value => ({ ...value, end: event.target.value })); }} /></label></div>
      {periodError && <p className="jc-error" role="alert">{periodError}</p>}
      {questions.map(([key, title, placeholder]) => <label key={key}>{title}<textarea aria-label={title} rows={3} maxLength={2000} placeholder={placeholder} value={fields[key] ?? ''} onChange={event => { setFields(value => ({ ...value, [key]: event.target.value })); }} /><span className="jc-print-text">{fields[key]?.trim() ? fields[key] : '未記入'}</span></label>)}
      <SavedNoteBar note={note} dirty={dirty} onSave={() => { void note.save(serializeReview(draft)); }} />
    </section>
  </section>;
}
