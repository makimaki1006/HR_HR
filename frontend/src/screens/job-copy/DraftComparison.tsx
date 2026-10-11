import { useState } from 'react';
import type { DraftSnapshot } from '../../generated/DraftSnapshot';
import type { DraftStatus } from '../../generated/DraftStatus';
import type { JobCopyRecord } from './data';
import { draftStateLabel } from './hubspotListings';
import { useDraftMutation } from './draftApi';
export function DraftReview({ job, draft, onSaved }: { job: JobCopyRecord; draft: DraftSnapshot; onSaved: (draft: DraftSnapshot, revision: string) => void }) {
  const [status, setStatus] = useState<DraftStatus>(draft.review_status);
  const mutation = useDraftMutation(onSaved);
  const latest = job.latestDraftId === draft.draft_id;
  const source = { csv: 'CSV', pdf: 'PDF', excel: 'Excel', free_text: '文章', url: 'URL' }[draft.source_kind];
  return <section className="jc-draft-review" aria-label="案の確認状態"><h3>{draftStateLabel(draft.review_status)}の案</h3><p>元データ：{source}。確認状態を変更しても、掲載中の本文は更新しません。</p>{!latest ? <p>過去の案です。確認状態を変更できるのは最新の案だけです。</p> : !job.canWriteDrafts ? <p>確認状態を変更する権限または利用設定がありません。</p> : <div className="jc-draft-review-controls"><label><span>案の確認状態</span><select value={status} disabled={mutation.busy || mutation.uncertain} onChange={event => { setStatus(event.target.value as DraftStatus); }}><option value="pending">確認待ち</option><option value="adopted">採用</option><option value="rejected">見送り</option></select></label><button type="button" className="jc-button" disabled={mutation.busy || (!mutation.uncertain && status === draft.review_status)} onClick={() => { if (job.hubspotId && job.draftRevision) void mutation.send(`/api/job-copy/listings/${encodeURIComponent(job.hubspotId)}/draft`, 'patch', { draft_id: draft.draft_id, base_revision: job.draftRevision, status }); }}>{mutation.uncertain ? '保存結果を確認' : '確認状態を保存'}</button></div>}<details className="jc-draft-source"><summary>元データを見る</summary><pre className="jc-body">{draft.source_text ?? "未取得"}</pre></details>{mutation.message && <p role="status">{mutation.message}</p>}</section>;
}
