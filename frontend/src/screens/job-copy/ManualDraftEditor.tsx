import { useState } from 'react';
import type { CopyVersion, JobCopyRecord } from './data';
import type { DraftSnapshot } from '../../generated/DraftSnapshot';
import type { ManualDraftRequest } from '../../generated/ManualDraftRequest';
import { useDraftMutation } from './draftApi';
import { ColumnComparison, compareColumns } from './ColumnComparison';
import { manualColumns, managementColumns, manualLabel, manualRow, conditionChanges, bodyHash, rowBody } from './manualEditModel';
export function ManualDraftEditor({ job, current, base, onSaved, onCancel }: { job: JobCopyRecord; current: CopyVersion; base: CopyVersion; onSaved: (draft: DraftSnapshot, revision: string) => void; onCancel: () => void }) {
  const currentRow = job.currentDraftRow ?? manualRow(current.comparisonBody ?? current.body, job.media === 'HRハッカー');
  const baseline = base.draft?.row ?? currentRow;
  const [row, setRow] = useState<Record<string, string>>(() => ({ ...baseline, ...Object.fromEntries(managementColumns.map(key => [key, currentRow[key] ?? ''])) }));
  const [reason, setReason] = useState('');
  const [confirmed, setConfirmed] = useState(false);
  const [preparing, setPreparing] = useState(false);
  const [error, setError] = useState('');
  const mutation = useDraftMutation(onSaved);
  const changes = conditionChanges(currentRow, baseline, row);
  async function save() {
    if (!job.hubspotId || !job.draftRevision || (!mutation.uncertain && (!confirmed || (changes.length > 0 && !reason.trim())))) return;
    setPreparing(true); setError('');
    try {
      const request: Omit<ManualDraftRequest, 'operation_id'> = { base_revision: job.draftRevision, base_body_hash: await bodyHash(base.draft ? rowBody(base.draft.row) : base.comparisonBody ?? base.body), base_draft_id: base.draft?.draft_id ?? null, row, reason };
      await mutation.send(`/api/job-copy/listings/${encodeURIComponent(job.hubspotId)}/manual-draft`, 'post', request);
    } catch { setError('保存の準備ができませんでした。時間を置いてもう一度お試しください。'); }
    finally { setPreparing(false); }
  }
  return <section className="jc-manual-editor" aria-label="求人の手直し"><h2>求人の文面を手直しする</h2><p>基にする文面：{base.label}。保存後、現在の文面と比べて確認します。入力していない項目は空欄のまま保存します。</p>
    {changes.length > 0 && <div className="jc-condition-warning" role="alert"><h3>募集条件に関わる変更があります</h3><p>給与・勤務地・勤務時間・雇用などの項目が変わっています。事実を確認し、理由を入力してください。</p><ul>{changes.map(key => <li key={key}>{manualLabel(key)}</li>)}</ul><label>条件を変更する理由<textarea value={reason} maxLength={2000} rows={3} disabled={mutation.busy || mutation.uncertain} onChange={event => { setReason(event.target.value); }} /></label><p>理由を入力してから確認待ちの案として保存してください。</p></div>}
    <fieldset disabled={preparing || mutation.busy || mutation.uncertain}><legend>求人票の84項目</legend><details className="jc-management-fields"><summary>管理用の項目（この項目は変更しません）</summary>{managementColumns.map(key => <p key={key}>{manualLabel(key)}：この項目は変更しません</p>)}</details><div className="jc-manual-fields">{manualColumns.filter(key => !managementColumns.includes(key)).map(key => <label key={key}><span>{manualLabel(key)}</span><textarea aria-label={manualLabel(key)} rows={['仕事内容', '制作メモ'].includes(key) || key.endsWith('内容') ? 4 : 2} value={row[key] ?? ''} maxLength={65000} placeholder="（空欄）" onChange={event => { setConfirmed(false); setRow(previous => ({ ...previous, [key]: event.target.value })); }} /></label>)}</div></fieldset>
    <section aria-label="保存前の変更確認"><h3>保存前に変更内容を確認</h3><p>今の文面から変わった項目</p><ColumnComparison result={compareColumns(rowBody(currentRow), rowBody(row), currentRow, row)} />{base.draft && <><p>基にした案から変わった項目</p><ColumnComparison result={compareColumns(rowBody(baseline), rowBody(row), baseline, row)} /></>}<label className="jc-manual-confirmation"><input type="checkbox" checked={confirmed} disabled={preparing || mutation.busy || mutation.uncertain} onChange={event => { setConfirmed(event.target.checked); }} />変更した項目と内容を確認しました</label></section>
    <div className="jc-flow-actions"><button className="jc-button jc-primary" disabled={!job.canWriteDrafts || preparing || mutation.busy || (!mutation.uncertain && (!confirmed || (changes.length > 0 && !reason.trim())))} onClick={() => { void save(); }}>{mutation.uncertain ? '保存結果を確認' : '確認待ちの案として保存して比較'}</button><button className="jc-button" disabled={preparing || mutation.busy || mutation.uncertain} onClick={onCancel}>手直しを閉じる</button></div>{(mutation.message || error) && <p role="status">{error || mutation.message}</p>}
  </section>;
}
