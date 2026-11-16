import { createContext, useContext, useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { apiGet } from '../../api/client';
import { DisabledOwnerFilter, OwnerFilter, OwnerFilterPlacement, PendingOwnerFilter, ownerParams } from './OwnerTodo';
import { MoreActionsProvider, MoreMenu } from './MoreMenu';
import { backoff, runWhenDue } from '../../shared/preparingRetry';
import { recheckSelfOwner } from './selfOwner';
import type { MoreActions } from './MoreMenu';
import type { WorkReady } from './OwnerTodo';
import { updateJobCopyQuery, useJobCopySearch } from './navigation';
import { originMenuView, originOf } from './origin';
import { publishOwnerNames } from './ownerNames';
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
    const controller = new AbortController(); const wait = backoff(2000);
    const load = async () => {
      const r = await apiGet<ConsultantScope & { status: string }>(`/api/job-copy/consultant-scope?${ownerQuery}`, { signal: controller.signal });
      if (controller.signal.aborted) return;
      if (!r.ok || r.data.status === 'failed') { setError(true); return; }
      // 形の違う応答（担当者の一覧が無い等）で画面全体が落ちないよう、読み込めなかった扱いにする。
      if (r.data.status === 'ready' && !Array.isArray(r.data.owners)) { setError(true); return; }
      if (r.data.status !== 'ready') { runWhenDue(controller.signal, wait(), () => { void load(); }); return; }
      setScope(r.data); setError(false); recheckSelfOwner();
      if (!r.data.notice && [null, 'me'].includes(new URLSearchParams(ownerQuery).get('owner'))) updateJobCopyQuery({ owner: r.data.owner, owner_field: r.data.owner_field }, 'replace');
    };
    void load(); return () => { controller.abort(); };
  }, [ownerQuery, reload, demo]);
  useEffect(() => { if (scope) publishOwnerNames(scope.owners); }, [scope]);
  // 一覧が届くまでは短い 1 行だけを出す（選択欄を先に出すと上の帯が高くなり、一覧に見える枚数が減る）。
  return scope ? <OwnerFilter work={scope} /> : error ? <p role="alert">担当者を読み込めませんでした。<button onClick={() => { setReload(n => n + 1); }}>担当者を再読み込み</button></p> : <PendingOwnerFilter />;
}
const ConsultantHeaderContent = createContext<ReactNode>(null);
export function ConsultantMenuHeader() { return useContext(ConsultantHeaderContent); }
export function ConsultantMenu({ children, inlineHeader = false, onNavigate }: { children: ReactNode; inlineHeader?: boolean; onNavigate?: () => void }) {
  const query = new URLSearchParams(useJobCopySearch()); const selected = query.get('view');
  // 案件・求人のページは、来た元のメニュー（案件一覧・危ない案件など）を選んだ表示にする。
  const highlighted = selected ?? (['deal', 'listing'].some(key => query.has(key)) ? originMenuView(originOf(`?${query.toString()}`)) : null);
  // 今日・案件一覧の担当情報は、その画面が取得した応答を共通欄に表示する。
  const nativeOwner = selected === 'deals' || (!selected && !['company', 'deal', 'listing', 'job'].some(key => query.has(key)) && !['fixed'].includes(query.get('source') ?? '') && query.get('demo') !== '1' && query.get('data') !== 'actual');
  // 担当の欄と「⋯ その他」は、どの画面でも同じ場所（帯の右）に出す。担当で絞れない画面では押せない欄と理由を出す。
  // 職種の指定を案件（deal）で開いている間は、担当では絞らない。
  const dealOnly = selected === 'occupations' && !!query.get('deal')?.trim();
  const [ownerTarget, setOwnerTarget] = useState<HTMLDivElement | null>(null);
  const [moreActions, setMoreActions] = useState<MoreActions | null>(null);
  const goHome = (fixed: boolean) => { updateJobCopyQuery({ view: null, company: null, deal: null, listing: null, job: null, risk_deal: null, report: null, from: null, source: fixed ? 'fixed' : null, data: null, demo: null }); onNavigate?.(); };
  const owner = nativeOwner ? <div ref={setOwnerTarget} className="cm-owner" /> : selected === 'reasons' ? <DisabledOwnerFilter reason="全社の集計です" /> : dealOnly ? <DisabledOwnerFilter reason="案件で絞っている間は担当で絞りません" /> : <ConsultantScopeFilter />;
  const menu = <OwnerFilterPlacement value={undefined}><div className="cm-menu"><nav aria-label="案件管理のメニュー">
    {([[null, '今日'], ['risk', '危ない案件'], ['houjin', '顧客'], ['deals', '案件一覧'], ['occupations', '職種の指定'], ['reasons', '応募理由'], ['unlinked', '案件が決まっていないMTG録画']] as const).map(([view, label]) => <button type="button" key={label} aria-current={!query.has('report') && (highlighted === view || (view === 'deals' && highlighted === 'good')) ? 'page' : undefined} onClick={() => { updateJobCopyQuery({ view, company: null, deal: null, listing: null, job: null, risk_deal: null, report: null, from: null, ...(view === null ? { source: null, data: null, demo: null } : {}) }); onNavigate?.(); }}>{label}</button>)}
  </nav><div className="cm-right">{owner}<MoreMenu actions={moreActions} onGoHome={goHome} /></div></div></OwnerFilterPlacement>;
  return <div className="jc-app cm-shell"><MoreActionsProvider value={setMoreActions}><ConsultantHeaderContent value={inlineHeader ? menu : null}>{!inlineHeader && menu}<OwnerFilterPlacement value={nativeOwner ? ownerTarget : null}>{children}</OwnerFilterPlacement></ConsultantHeaderContent></MoreActionsProvider></div>;
}
