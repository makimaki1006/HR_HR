import { useState } from 'react';
import type { CopyVersion, JobCopyRecord } from './data';
import type { DraftSnapshot } from '../../generated/DraftSnapshot';
import type { ManualDraftRequest } from '../../generated/ManualDraftRequest';
import { useDraftMutation } from './draftApi';
import { manualColumns, managementColumns, manualLabel, manualRow, conditionChanges, bodyHash, rowBody } from './manualEditModel';
export function ManualDraftEditor({ job, current, base, onSaved, onCancel }: { job: JobCopyRecord; current: CopyVersion; base: CopyVersion; onSaved: (draft: DraftSnapshot, revision: string) => void; onCancel: () => void }) {
  const currentRow = job.currentDraftRow ?? manualRow(current.comparisonBody ?? current.body);
  const baseline = base.draft?.row ?? currentRow;
  const [row, setRow] = useState<Record<string, string>>(() => ({ ...baseline, ...Object.fromEntries(managementColumns.map(key => [key, currentRow[key] ?? ''])) }));
  const [reason, setReason] = useState('');
  const [preparing, setPreparing] = useState(false);
  const [error, setError] = useState('');
  const mutation = useDraftMutation(onSaved);
  const changes = conditionChanges(currentRow, baseline, row);
  async function save() {
    if (!job.hubspotId || !job.draftRevision) return;
    setPreparing(true); setError('');
    try {
      const request: Omit<ManualDraftRequest, 'operation_id'> = { base_revision: job.draftRevision, base_body_hash: await bodyHash(base.draft ? rowBody(base.draft.row) : base.comparisonBody ?? base.body), base_draft_id: base.draft?.draft_id ?? null, row, reason };
      await mutation.send(`/api/job-copy/listings/${encodeURIComponent(job.hubspotId)}/manual-draft`, 'post', request);
    } catch { setError('保存の準備ができませんでした。時間を置いてもう一度お試しください。'); }
    finally { setPreparing(false); }
  }
  return <section className="jc-manual-editor" aria-label="求人の手直し"><h2>求人の文面を手直しする</h2><p>基にする文面：{base.label}。保存後、今の版と比べて確認します。空欄は未取得として扱います。</p>
    {changes.length > 0 && <div className="jc-condition-warning" role="alert"><h3>募集条件に関わる変更があります</h3><p>給与・勤務地・勤務時間・雇用・休日などは、事実の確認が必要です。文章中の条件も確認してください。</p><ul>{changes.map(key => <li key={key}>{manualLabel(key)}</li>)}</ul><label>条件を変更する理由<textarea value={reason} maxLength={2000} rows={3} disabled={mutation.busy || mutation.uncertain} onChange={event => { setReason(event.target.value); }} /></label><p>理由を入力してから確認待ちの案として保存してください。</p></div>}
    <fieldset disabled={preparing || mutation.busy || mutation.uncertain}><legend>求人票の84項目</legend><div className="jc-manual-fields">{manualColumns.map(key => <label key={key}><span>{manualLabel(key)}</span>{managementColumns.includes(key) ? <input aria-label={manualLabel(key)} type="password" readOnly value="" placeholder="非表示・取得した値を保持" /> : <textarea aria-label={manualLabel(key)} rows={['仕事内容', '制作メモ'].includes(key) || key.endsWith('内容') ? 4 : 2} value={row[key] ?? ''} maxLength={65000} placeholder="未取得" onChange={event => { setRow(previous => ({ ...previous, [key]: event.target.value })); }} />}</label>)}</div></fieldset>
    <div className="jc-flow-actions"><button className="jc-button jc-primary" disabled={!job.canWriteDrafts || preparing || mutation.busy || (!mutation.uncertain && changes.length > 0 && !reason.trim())} onClick={() => { void save(); }}>{mutation.uncertain ? '保存結果を確認' : '確認待ちの案として保存して比較'}</button><button className="jc-button" disabled={preparing || mutation.busy || mutation.uncertain} onClick={onCancel}>手直しを閉じる</button></div>{(mutation.message || error) && <p role="status">{error || mutation.message}</p>}
  </section>;
}
