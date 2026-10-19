import { createContext, useContext, useEffect, useId, useState } from 'react';
import type { ReactNode } from 'react';
import { apiGet } from '../../api/client';
import { OwnerFilter, OwnerFilterPlacement, ownerParams } from './OwnerTodo';
import type { WorkReady } from './OwnerTodo';
import { updateJobCopyQuery, useJobCopySearch } from './navigation';
import './consultant-risk.css';
import './home.css';
import { demoOwnerTodo } from './demoOwnerTodo';
export type ConsultantScope = Pick<WorkReady, 'owner' | 'owner_field' | 'notice' | 'owners'>;
function ConsultantScopeFilter() {
  const search = useJobCopySearch(); const query = new URLSearchParams(search);
  const demo = query.get('demo') === '1';
  const ownerQuery = ownerParams(search);
  const [scope, setScope] = useState<ConsultantScope | null>(demo ? demoOwnerTodo : null);
  const [error, setError] = useState(false);
  const [reload, setReload] = useState(0);
  useEffect(() => {
    if (demo) return;
    const controller = new AbortController(); let timer: ReturnType<typeof setTimeout>;
    const load = async () => {
      const r = await apiGet<ConsultantScope & { status: string }>(`/api/job-copy/consultant-scope?${ownerQuery}`, { signal: controller.signal });
      if (controller.signal.aborted) return;
      if (!r.ok || r.data.status === 'failed') { setError(true); return; }
      if (r.data.status !== 'ready') { timer = setTimeout(() => { void load(); }, 2000); return; }
      setScope(r.data); setError(false);
      if (!r.data.notice && [null, 'me'].includes(new URLSearchParams(ownerQuery).get('owner'))) updateJobCopyQuery({ owner: r.data.owner, owner_field: r.data.owner_field }, 'replace');
    };
    void load(); return () => { controller.abort(); clearTimeout(timer); };
  }, [ownerQuery, reload, demo]);
  return scope ? <OwnerFilter work={scope} /> : <p role={error ? 'alert' : 'status'}>{error ? '担当者を読み込めませんでした。' : '担当者を確認しています…'}{error && <button onClick={() => { setReload(n => n + 1); }}>担当者を再読み込み</button>}</p>;
}
const ConsultantHeaderContent = createContext<ReactNode>(null);
export function ConsultantMenuHeader() { return useContext(ConsultantHeaderContent); }
export function ConsultantMenu({ children, inlineHeader = false, onNavigate }: { children: ReactNode; inlineHeader?: boolean; onNavigate?: () => void }) {
  const query = new URLSearchParams(useJobCopySearch()); const selected = query.get('view');
  // ホーム・案件一覧の担当情報は、その画面が取得した応答を共通欄に表示する。
  const nativeOwner = selected === 'deals' || (!selected && !['company', 'deal', 'listing', 'job'].some(key => query.has(key)) && !['fixed'].includes(query.get('source') ?? '') && query.get('demo') !== '1' && query.get('data') !== 'actual');
  const fullOwner = nativeOwner || selected === 'risk' || selected === 'good';
  const [ownerTarget, setOwnerTarget] = useState<HTMLDivElement | null>(null);
  const [expanded, setExpanded] = useState(false);
  const ownerPopoverId = useId();
  const [popoverTop, setPopoverTop] = useState(0);
  const menu = <OwnerFilterPlacement value={undefined}><div className="cm-menu"><nav aria-label="案件管理のメニュー">
    {([[null, 'ホーム'], ['risk', '危ない案件'], ['good', 'いい案件'], ['deals', '案件一覧']] as const).map(([view, label]) => <button type="button" key={label} aria-current={selected === view ? 'page' : undefined} onClick={() => { updateJobCopyQuery({ view, company: null, deal: null, listing: null, job: null, risk_deal: null, ...(view === null ? { source: null, data: null, demo: null } : {}) }); onNavigate?.(); }}>{label}</button>)}
    <a href="/consulting#deal/team">コンサルKPI</a>
    {!fullOwner && <div className="cm-owner-toggle"><button type="button" popoverTarget={ownerPopoverId} aria-controls={ownerPopoverId} aria-expanded={expanded} onClick={event => { setPopoverTop(event.currentTarget.getBoundingClientRect().bottom + 8); }}>担当者で絞る</button><div id={ownerPopoverId} popover="auto" className="cm-owner-popover" role="region" aria-label="担当者の選択" style={{ top: popoverTop }} onToggle={event => { setExpanded(event.newState === 'open'); }}>{expanded && <ConsultantScopeFilter />}</div></div>}
  </nav>{nativeOwner ? <div ref={setOwnerTarget} className="cm-owner" /> : fullOwner && <ConsultantScopeFilter />}</div></OwnerFilterPlacement>;
  return <div className="jc-app cm-shell"><ConsultantHeaderContent value={inlineHeader ? menu : null}>{!inlineHeader && menu}<OwnerFilterPlacement value={nativeOwner ? ownerTarget : null}>{children}</OwnerFilterPlacement></ConsultantHeaderContent></div>;
}
