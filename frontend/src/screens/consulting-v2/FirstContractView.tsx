import { useState } from 'react';
import { ConsultantSelect } from './BoardView';
import { Async, Fresh, HubspotLink, Section } from './parts';
import type { Route } from './route';
import type { BoardMeta } from './types';
import { useApi } from './useApi';

export const FIRST_GATE_HEADERS = ['初回MTG（7日以内）', '求人掲載（14日以内）', '応募1件（30日以内）', '面接1件（段階3まで）', '満足度の回答（段階3まで）'];

export function firstGateStatus(g: FirstGate | undefined): { text: string; tone: string } {
  if (g?.achieved === true) return { text: '達成', tone: 'achieved' };
  if (g?.check) return { text: '確かめてください（開始前のMTGあり）', tone: 'unknown' };
  if (g?.not_started) return { text: '開始前', tone: 'unknown' };
  // 間に合う最後の日（段階3の関門は境目の前日）。古い応答で無ければ期限日を使う
  const day = g?.last_day ?? g?.deadline;
  if (g?.achieved !== false || !day || !/^\d{4}-\d{2}-\d{2}$/.test(day)) return { text: '判定できない', tone: 'unknown' };
  const date = `${String(Number(day.slice(5, 7)))}/${String(Number(day.slice(8, 10)))}`;
  return g.overdue ? { text: `期限超過（${date}）`, tone: 'overdue' } : { text: `期限前（${date}まで）`, tone: 'pending' };
}

export interface FirstGate {
  label: string;
  achieved: boolean | null;
  deadline: string | null;
  last_day?: string | null;
  overdue: boolean;
  not_started?: boolean;
  check?: boolean;
  text: string;
}
export interface FirstRow {
  deal_id: string;
  name: string;
  consultant: string;
  start: string;
  expiry: string | null;
  first_contract: { gates: FirstGate[]; overdue_count: number; reason?: string };
  goals?: { failed: boolean; ideal?: string; minimum?: string; draft?: boolean };
}
export interface FirstResponse {
  goals_available?: boolean;
  sales_unavailable?: boolean;
  sales_failed_count?: number;
  sales_empty_count?: number;
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
          <div className='cv2-table-wrap'><table className='cv2-table' aria-label={title}>
            <thead><tr><th>案件</th><th>担当</th><th>開始〜満了</th>{FIRST_GATE_HEADERS.map((label) => <th key={label}>{label}</th>)}<th>初回MTGの目標</th></tr></thead>
            <tbody>{rows.map((r) => <tr key={r.deal_id} onClick={() => { go({ view: 'detail', id: r.deal_id }); }}>
              <td><button type='button' className='cv2-link' onClick={(e) => { e.stopPropagation(); go({ view: 'detail', id: r.deal_id }); }}>{r.name}</button> <span onClick={(e) => { e.stopPropagation(); }}><HubspotLink dealId={r.deal_id} portal={d.meta.hubspot_portal_id} /></span></td>
              <td>{r.consultant || '担当未設定'}</td><td>{r.start || '開始日不明'}〜{r.expiry ?? '満了日不明'}</td>
              {FIRST_GATE_HEADERS.map((label, index) => {
                const status = firstGateStatus(r.first_contract.gates[index]);
                return <td key={label} className={`cv2-first-gate cv2-first-${status.tone}`}>{status.text}</td>;
              })}
              <td>{d.goals_available === false ? '—' : !r.goals || r.goals.failed ? '読めませんでした' : <>{r.goals.draft && <span>案 · </span>}マインド：{r.goals.ideal}<br />コミット：{r.goals.minimum}</>}</td>
            </tr>)}</tbody>
          </table></div>
          {rows.length === 0 && <p>該当する契約は0件です。</p>}
        </Section>;
      };
      return <>
        <Fresh meta={d.meta} />
        <div className='cv2-filters'><ConsultantSelect value={route.c} names={d.consultants} onChange={(c) => { go({ c }); }} /></div>
        <p className='cv2-dim'>対象は稼働中で売上回数が1回の契約です。</p>
        {d.sales_unavailable && <p role='status'>売上回数を読めませんでした。少し待って読み直してください</p>}
        {!d.sales_unavailable && (d.sales_failed_count ?? 0) > 0 && <p className='cv2-dim'>売上回数を読めず対象から外した稼働中の契約 {d.sales_failed_count} 件</p>}
        {(d.sales_empty_count ?? 0) > 0 && <p className='cv2-dim'>売上回数が空の稼働中の契約 {d.sales_empty_count} 件</p>}
        <p className='cv2-dim cv2-first-note'>期限は暫定です。</p>
        <p className='cv2-dim'>期限超過が多い順、同じなら開始日が古い順です。</p>
        {d.goals_available === false && <p className='cv2-note'>目標は社内の Google ログインで見られます</p>}
        {table(d.rows, '初回契約')}
        {table(d.not_started, '開始前の初回契約')}
      </>;
    }}
  </Async>;
}
