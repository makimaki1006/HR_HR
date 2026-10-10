import { useState } from 'react';
import { HubSpotListingsPanel } from '../job-copy/HubSpotListingsPanel';
import type { JobCopyRecord } from '../job-copy/data';
import { useDraftMutation } from '../job-copy/draftApi';
import type { PipelineState } from './state';
export function DraftSavePanel({ s }: { s: PipelineState }) {
  const [selecting, setSelecting] = useState(false);
  const [job, setJob] = useState<JobCopyRecord | null>(null);
  const [saved, setSaved] = useState(false);
  const mutation = useDraftMutation((_, revision) => { setSaved(true); setJob(current => current ? { ...current, draftRevision: revision } : current); });
  const ready = Boolean(s.hrhacker && s.hrhackerCreatedAt && s.facts && !s.running && !s.normalizing && (s.status.hrhacker === 'done' || s.status.hrhacker === 'review'));
  return <section className="panel jg-draft-save" aria-label="作った案を求人に保存"><h2>作った案を求人に保存</h2><p>保存先の求人を1件選んでください。84項目の内容と抜き出した事実を、確認待ちの案として保存します。掲載中の本文は更新しません。</p><button type="button" className="btn" aria-expanded={selecting} disabled={mutation.busy || mutation.uncertain} onClick={() => { setSelecting(open => !open); }}>保存先の求人を選ぶ</button>{selecting && <fieldset disabled={mutation.busy || mutation.uncertain}><HubSpotListingsPanel onOpen={selected => { setJob(selected); setSaved(false); }} onLoading={() => { setJob(null); setSaved(false); }} onFailure={() => { setJob(null); }} /></fieldset>}{job && <div className="jg-draft-target"><h3>保存先：{job.title}</h3><p>{job.media} · {job.location}</p>{!job.canWriteDrafts && <p>案を保存する権限または利用設定がありません。</p>}<button type="button" className="btn" disabled={mutation.busy || (!mutation.uncertain && (!ready || !job.canWriteDrafts || !job.draftRevision))} onClick={() => { if (s.hrhacker && s.facts && job.hubspotId) { setSaved(false); void mutation.send(`/api/job-copy/listings/${encodeURIComponent(job.hubspotId)}/draft`, 'post', { base_revision: job.draftRevision, row: s.hrhacker.row, created_at: s.hrhackerCreatedAt, facts: s.facts, source_text: s.sourceText, source_kind: s.sourceKind === 'html' ? 'free_text' : s.sourceKind }); } }}>{mutation.uncertain ? '保存結果を確認' : 'この求人に案を保存'}</button>{!ready && <p>元データと案を確認し、必要な工程を再実行してから保存してください。</p>}{saved && <p><a href={`/app/job-copy?listing=${encodeURIComponent(job.hubspotId ?? '')}`}>求人文面管理で今の版と案を比べる</a></p>}</div>}{mutation.message && <p role="status">{mutation.message}</p>}</section>;
}
