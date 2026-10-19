import { useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { apiGet } from '../../api/client';
import { OwnerFilter, OwnerFilterPlacement, ownerParams } from './OwnerTodo';
import type { WorkReady } from './OwnerTodo';
import { updateJobCopyQuery, useJobCopySearch } from './navigation';
import './consultant-risk.css';
import './home.css';
import { demoOwnerTodo } from './demoOwnerTodo';
export type ConsultantScope = Pick<WorkReady, 'owner' | 'owner_field' | 'notice' | 'owners'>;
export function ConsultantMenu({ children }: { children: ReactNode }) {
  const search = useJobCopySearch(); const query = new URLSearchParams(search); const selected = query.get('view');
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
  return <><div className="jc-app cm-menu"><nav aria-label="案件管理のメニュー">
    {([[null, 'ホーム'], ['risk', '危ない案件'], ['good', 'いい案件'], ['deals', '案件一覧']] as const).map(([view, label]) => <button type="button" key={label} aria-current={selected === view ? 'page' : undefined} onClick={() => { updateJobCopyQuery({ view, company: null, deal: null, listing: null, job: null, risk_deal: null }); }}>{label}</button>)}
    <a href="/consulting#deal/team">担当者ごとの集計（コンサルKPI）</a>
  </nav>{scope ? <OwnerFilter work={scope} /> : <p role={error ? 'alert' : 'status'}>{error ? '担当者を読み込めませんでした。' : '担当者を確認しています…'}{error && <button onClick={() => { setReload(n => n + 1); }}>再読み込み</button>}</p>}</div><OwnerFilterPlacement value={true}>{children}</OwnerFilterPlacement></>;
}
