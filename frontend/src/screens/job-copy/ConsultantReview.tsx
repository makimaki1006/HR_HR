import { useEffect, useRef } from 'react';
import type { JobCopyRecord } from './data';
import { ApplicantComposition } from './ApplicantComposition';
import { observationWindowError } from './consultantReviewModel';
import { reportUrl } from './navigation';
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
  const questions: [string, string, string][] = [
    ['hypothesis', '仮説', '何を変えると、どの応募者層に届くと考えるか'],
    ['criterion', '評価基準', '対象層の応募件数・構成比など、期間とともに記入'],
    ['findings', '結果と根拠', '対象の本文・期間・集計値・取得できなかった情報'],
    ['action', '次に試すこと', '継続・変更・追加で確認することと、担当・期限'],
  ];
  const reportTarget = persistableListingId(job);
  return <section className="jc-consultant" aria-label="検証記録">
    <p className="jc-report-moved">{job.title} · {job.media}。顧客への報告は、スライドの画面で作ります。{reportTarget && <a className="jc-button jc-no-print" href={reportUrl({ scope: 'listing', id: reportTarget })} target="_blank" rel="noopener">報告書を作る（新しいタブ）</a>}</p>
    <ApplicantComposition job={job} billing={billing} selection={selection} onSelectionChange={value => { onDraft({ ...draft, selection: value }); }} />
    <section className="jc-cycle" id="job-copy-review-cycle"><h3>記録欄</h3><p>仮説 → 実施 → 結果の確認 → 次の施策を残すための記録欄です。{persisted ? '「保存」を押すと、この求人の記録として残り、案件管理を開ける人に表示されます。' : '入力はこの画面の中だけに残り、求人の切り替え・再読み込みで消えます。'}HubSpotには保存しません。</p>
      <div className="jc-cycle-controls"><label>進行状況<select value={stage} onChange={event => { onDraft({ ...draft, stage: event.target.value }); }}><option value="plan">Plan · 仮説と計画</option><option value="do">Do · 掲載変更を実施</option><option value="check">Check · 結果を確認</option><option value="act">Act · 次の施策を決定</option></select></label><label>変更対象<select value={target} onChange={event => { onDraft({ ...draft, target: event.target.value }); }}><option value="body">本文</option><option value="image">画像</option><option value="both">本文と画像</option></select></label>
      <label>確認開始日<input type="date" value={fields.start ?? ''} onChange={event => { setFields(value => ({ ...value, start: event.target.value })); }} /></label><label>確認終了日<input type="date" min={fields.start === '' ? undefined : fields.start} value={fields.end ?? ''} onChange={event => { setFields(value => ({ ...value, end: event.target.value })); }} /></label></div>
      {periodError && <p className="jc-error" role="alert">{periodError}</p>}
      {questions.map(([key, title, placeholder]) => <label key={key}>{title}<textarea aria-label={title} rows={3} maxLength={2000} placeholder={placeholder} value={fields[key] ?? ''} onChange={event => { setFields(value => ({ ...value, [key]: event.target.value })); }} /><span className="jc-print-text">{fields[key]?.trim() ? fields[key] : '未記入'}</span></label>)}
      <SavedNoteBar note={note} dirty={dirty} onSave={() => { void note.save(serializeReview(draft)); }} />
    </section>
  </section>;
}
