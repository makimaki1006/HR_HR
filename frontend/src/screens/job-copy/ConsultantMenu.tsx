import { createContext, useContext, useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { apiGet } from '../../api/client';
import { OwnerFilter, OwnerFilterPlacement, PendingOwnerFilter, ownerParams } from './OwnerTodo';
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
    const controller = new AbortController(); let timer: ReturnType<typeof setTimeout>; let attempt = 0;
    const load = async () => {
      const r = await apiGet<ConsultantScope & { status: string }>(`/api/job-copy/consultant-scope?${ownerQuery}`, { signal: controller.signal });
      if (controller.signal.aborted) return;
      if (!r.ok || r.data.status === 'failed') { setError(true); return; }
      // 形の違う応答（担当者の一覧が無い等）で画面全体が落ちないよう、読み込めなかった扱いにする。
      if (r.data.status === 'ready' && !Array.isArray(r.data.owners)) { setError(true); return; }
      if (r.data.status !== 'ready') { attempt += 1; timer = setTimeout(() => { void load(); }, Math.min(10_000, 2000 * attempt)); return; }
      setScope(r.data); setError(false);
      if (!r.data.notice && [null, 'me'].includes(new URLSearchParams(ownerQuery).get('owner'))) updateJobCopyQuery({ owner: r.data.owner, owner_field: r.data.owner_field }, 'replace');
    };
    void load(); return () => { controller.abort(); clearTimeout(timer); };
  }, [ownerQuery, reload, demo]);
  // 一覧が届くまでは短い 1 行だけを出す（選択欄を先に出すと上の帯が高くなり、一覧に見える枚数が減る）。
  return scope ? <OwnerFilter work={scope} /> : error ? <p role="alert">担当者を読み込めませんでした。<button onClick={() => { setReload(n => n + 1); }}>担当者を再読み込み</button></p> : <PendingOwnerFilter />;
}
const ConsultantHeaderContent = createContext<ReactNode>(null);
export function ConsultantMenuHeader() { return useContext(ConsultantHeaderContent); }
export function ConsultantMenu({ children, inlineHeader = false, onNavigate }: { children: ReactNode; inlineHeader?: boolean; onNavigate?: () => void }) {
  const query = new URLSearchParams(useJobCopySearch()); const selected = query.get('view');
  // 今日・案件一覧の担当情報は、その画面が取得した応答を共通欄に表示する。
  const nativeOwner = selected === 'deals' || (!selected && !['company', 'deal', 'listing', 'job'].some(key => query.has(key)) && !['fixed'].includes(query.get('source') ?? '') && query.get('demo') !== '1' && query.get('data') !== 'actual');
  // 担当の選択は「いつも見るもの」として、どの画面でも同じ形で手元に出す（応募理由だけは担当で絞らないので出さない）。
  const fullOwner = selected !== 'reasons';
  const [ownerTarget, setOwnerTarget] = useState<HTMLDivElement | null>(null);
  const menu = <OwnerFilterPlacement value={undefined}><div className="cm-menu"><nav aria-label="案件管理のメニュー">
    {([[null, '今日'], ['risk', '危ない案件'], ['houjin', '法人'], ['deals', '案件一覧'], ['reasons', '応募理由']] as const).map(([view, label]) => <button type="button" key={label} aria-current={selected === view || (view === 'deals' && selected === 'good') ? 'page' : undefined} onClick={() => { updateJobCopyQuery({ view, company: null, deal: null, listing: null, job: null, risk_deal: null, ...(view === null ? { source: null, data: null, demo: null } : {}) }); onNavigate?.(); }}>{label}</button>)}
    <a href="/consulting#deal/team">コンサルKPI</a>
  </nav>{nativeOwner ? <div ref={setOwnerTarget} className="cm-owner" /> : fullOwner && <ConsultantScopeFilter />}</div></OwnerFilterPlacement>;
  return <div className="jc-app cm-shell"><ConsultantHeaderContent value={inlineHeader ? menu : null}>{!inlineHeader && menu}<OwnerFilterPlacement value={nativeOwner ? ownerTarget : null}>{children}</OwnerFilterPlacement></ConsultantHeaderContent></div>;
}
