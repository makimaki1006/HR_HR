import { useState } from 'react';
import type { DraftSnapshot } from '../../generated/DraftSnapshot';
import type { DraftStatus } from '../../generated/DraftStatus';
import type { CopyVersion, JobCopyRecord } from './data';
import { draftStateLabel } from './hubspotListings';
import { useDraftMutation } from './draftApi';
import { parseSalaryText, sameSalary } from './salaryExtract';
const facts = [ ['salary', '給与', '給与'], ['working_hours', '勤務時間', '勤務時間'], ['holidays', '休日', '休日'], ['work_location', '勤務地', '勤務地'], ['employment_type', '雇用形態', '雇用形態'], ['insurance', '保険', '保険'], ['allowances', '手当', '手当'], ['required_qualifications', '応募資格', '応募資格'] ] as const;
const normalize = (text: string) => text.normalize('NFKC').replace(/\s/g, '');
export function factDifferences(draft: DraftSnapshot, current: CopyVersion | undefined, location: string) {
  return facts.flatMap(([key, label, heading]) => {
    const fact = draft.facts[key];
    if (fact?.status !== 'verified' || !fact.value.trim()) return [];
    const headings = key === 'holidays' ? ['休日', '休日・休暇'] : [heading];
    const value = current?.bodySections?.find(section => headings.includes(section.heading))?.text ?? (current?.body.match(new RegExp(`(?:^|\\n)(?:${headings.join('|')})[：:]([^\\n]+)`))?.[1] ?? (key === 'work_location' && location !== '勤務地不明' ? location : ''));
    const leftPay = key === 'salary' ? parseSalaryText(value) : null;
    const rightPay = key === 'salary' ? parseSalaryText(fact.value) : null;
    const equalPay = key === 'salary' && leftPay?.kind !== '不明' && leftPay?.min !== null && rightPay?.kind !== '不明' && rightPay?.min !== null && sameSalary(leftPay, rightPay);
    if (normalize(value) === normalize(fact.value) || equalPay) return [];
    return [{ key, label, fact: fact.value, current: value || '未取得', missing: !value, evidence: fact.evidence_quote }];
  });
}
export function DraftFacts({ draft, current, location }: { draft: DraftSnapshot; current: CopyVersion | undefined; location: string }) {
  const differences = factDifferences(draft, current, location);
  return <section className="jc-draft-facts" aria-label="元データの事実と今の版の確認"><h3>元データの事実と今の版の確認</h3><p>案に保存された事実を、今の版と比べています。掲載条件が変わったかどうかは、この比較だけでは判断できません。</p>{differences.length ? <table><thead><tr><th>確認する内容</th><th>今の版</th><th>元データの事実</th></tr></thead><tbody>{differences.map(item => <tr key={item.key}><th>{item.label}<small>{item.missing ? ' 今の版は未取得' : ' 違いあり'}</small></th><td>{item.current}</td><td>{item.fact}<details><summary>根拠を見る</summary>{item.evidence || '未取得'}</details></td></tr>)}</tbody></table> : <p>確認できた事実の違いはありません。未取得の事実は比べていません。</p>}</section>;
}
export function DraftReview({ job, draft, onSaved }: { job: JobCopyRecord; draft: DraftSnapshot; onSaved: (draft: DraftSnapshot, revision: string) => void }) {
  const [status, setStatus] = useState<DraftStatus>(draft.review_status);
  const mutation = useDraftMutation(onSaved);
  const latest = job.latestDraftId === draft.draft_id;
  const source = { csv: 'CSV', pdf: 'PDF', excel: 'Excel', free_text: '文章', url: 'URL' }[draft.source_kind];
  return <section className="jc-draft-review" aria-label="案の確認状態"><h3>{draftStateLabel(draft.review_status)}の案</h3><p>元データ：{source}。確認状態を変更しても、掲載中の本文は更新しません。</p>{!latest ? <p>過去の案です。確認状態を変更できるのは最新の案だけです。</p> : !job.canWriteDrafts ? <p>確認状態を変更する権限または利用設定がありません。</p> : <><label>案の確認状態<select value={status} disabled={mutation.busy || mutation.uncertain} onChange={event => { setStatus(event.target.value as DraftStatus); }}><option value="pending">確認待ち</option><option value="adopted">採用</option><option value="rejected">見送り</option></select></label><button type="button" className="jc-button" disabled={mutation.busy || (!mutation.uncertain && status === draft.review_status)} onClick={() => { if (job.hubspotId && job.draftRevision) void mutation.send(`/api/job-copy/listings/${encodeURIComponent(job.hubspotId)}/draft`, 'patch', { draft_id: draft.draft_id, base_revision: job.draftRevision, status }); }}>{mutation.uncertain ? '保存結果を確認' : '確認状態を保存'}</button></>}{mutation.message && <p role="status">{mutation.message}</p>}</section>;
}
