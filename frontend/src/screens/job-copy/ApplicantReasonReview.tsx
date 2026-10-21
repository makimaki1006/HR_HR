import { useState } from 'react';
import type { JobCopyRecord } from './data';
import { ApplicantReasons } from './ApplicantReasons';
import { ChangeReasons } from './ChangeReasons';

export type ReasonView = 'compare' | 'changes';

export function ApplicantReasonReview({ job, view = 'compare', onViewChange, onOpenFactors }: { job: JobCopyRecord; view?: ReasonView; onViewChange?: (view: ReasonView) => void; onOpenFactors?: () => void }) {
  const versions = job.versions.filter(version => version.kind === 'published');
  const [before, setBefore] = useState(versions[0]?.id ?? '');
  const [after, setAfter] = useState(versions.at(-1)?.id ?? '');
  const switcher = <div className="cr-view jc-no-print" role="group" aria-label="表示の切り替え">{([['compare', '記述の比較'], ['changes', '変更ごと']] as const).map(([value, label]) => <button key={value} type="button" className="jc-button" aria-pressed={view === value} onClick={() => { onViewChange?.(value); }}>{label}</button>)}</div>;
  if (view === 'changes') return <section className="jc-analysis" aria-label="応募理由の確認"><h2>応募理由を確認する</h2>{switcher}<ChangeReasons job={job} onOpenFactors={() => { onOpenFactors?.(); }} /></section>;
  return <section className="jc-analysis" aria-label="応募理由の確認"><h2>応募理由を確認する</h2>{switcher}
    <p>本文との対応が確認できた記述だけを比較します。対応不明の理由は別に表示します。</p>
    <div className="jc-analysis-controls jc-no-print"><label>理由比較元<select value={before} onChange={event => { setBefore(event.target.value); }}>{versions.length ? versions.map(version => <option key={version.id} value={version.id}>{version.label}</option>) : <option value="">掲載を確認できた本文未取得</option>}</select></label><label>理由比較先<select value={after} onChange={event => { setAfter(event.target.value); }}>{versions.length ? versions.map(version => <option key={version.id} value={version.id}>{version.label}</option>) : <option value="">掲載を確認できた本文未取得</option>}</select></label></div>
    <ApplicantReasons job={job} before={versions.find(version => version.id === before)} after={versions.find(version => version.id === after)} />
  </section>;
}
