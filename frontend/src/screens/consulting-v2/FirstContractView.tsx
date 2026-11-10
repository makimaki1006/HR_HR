import { useState } from 'react';
import { ConsultantSelect } from './BoardView';
import { Async, Fresh, HubspotLink, Section } from './parts';
import type { Route } from './route';
import type { BoardMeta } from './types';
import { useApi } from './useApi';

export interface FirstGate {
  label: string;
  achieved: boolean | null;
  deadline: string | null;
  overdue: boolean;
  text: string;
}
export interface FirstRow {
  deal_id: string;
  name: string;
  consultant: string;
  start: string;
  expiry: string | null;
  first_contract: { gates: FirstGate[]; overdue_count: number };
  goals: { failed: boolean; ideal?: string; minimum?: string; draft?: boolean };
}
export interface FirstResponse {
  meta: BoardMeta;
  consultants: string[];
  rows: FirstRow[];
  not_started: FirstRow[];
}

export function FirstContractView({ route, go }: { route: Route; go: (r: Partial<Route>) => void }) {
  const [reload, setReload] = useState(0);
  const result = useApi<FirstResponse>('/api/consulting/first-contract', reload);
  return <Async result={result} onRetry={() => { setReload((n) => n + 1); }}>
    {(d) => {
      const table = (source: FirstRow[], title: string) => {
        const rows = source.filter((r) => route.c === '' || r.consultant === route.c);
        return <Section title={`${title}（${String(rows.length)} 件）`}>
          <div className="cv2-table-wrap"><table className="cv2-table"><caption>{title}</caption>
            <thead><tr><th>案件</th><th>担当</th><th>開始〜満了</th><th>5つの関門</th><th>初回MTGの目標</th></tr></thead>
            <tbody>{rows.map((r) => <tr key={r.deal_id} onClick={() => { go({ view: 'detail', id: r.deal_id }); }}>
              <td><button type="button" className="cv2-link" onClick={(e) => { e.stopPropagation(); go({ view: 'detail', id: r.deal_id }); }}>{r.name}</button> <span onClick={(e) => { e.stopPropagation(); }}><HubspotLink dealId={r.deal_id} portal={d.meta.hubspot_portal_id} /></span></td>
              <td>{r.consultant || '担当未設定'}</td><td>{r.start || '開始日不明'}〜{r.expiry ?? '満了日不明'}</td>
              <td><span className="cv2-first-gates">{r.first_contract.gates.map((g) => <span key={g.label} tabIndex={0} onClick={(e) => { e.stopPropagation(); }} className={g.overdue ? 'cv2-first-overdue' : undefined}
                title={`${g.label}：${g.text}（期限 ${g.deadline ?? '不明'}）`} aria-label={`${g.label}：${g.text}（期限 ${g.deadline ?? '不明'}）`}>{g.achieved === true ? '●' : '○'}</span>)}</span></td>
              <td>{r.goals.failed ? '読めませんでした' : <>{r.goals.draft && <span>案 · </span>}理想：{r.goals.ideal}<br />前進ライン：{r.goals.minimum}</>}</td>
            </tr>)}</tbody>
          </table></div>
          {rows.length === 0 && <p>該当する契約は0件です。</p>}
        </Section>;
      };
      return <>
        <Fresh meta={d.meta} />
        <div className="cv2-filters"><ConsultantSelect value={route.c} names={d.consultants} onChange={(c) => { go({ c }); }} /></div>
        <p className="cv2-note">関門は左から、初回MTGの記録（7日以内）、求人掲載（14日以内）、応募1件（30日以内）、面接1件、満足度の回答です。面接と満足度は段階3に入るまでが期限です。期限は暫定です。</p>
        <p className="cv2-dim">● 達成 / ○ 未達・判定できない。赤は期限超過です。各丸に重ねると判定と期限が読めます。期限超過が多い順、同じなら開始日が古い順です。</p>
        {table(d.rows, '初回契約')}
        {table(d.not_started, '開始前の初回契約')}
      </>;
    }}
  </Async>;
}
