import { StageStay } from './SupplementalViews';
import { useState } from 'react';
import { ConsultantSelect, filterRows } from './BoardView';
import { DealTable, TODAY_COLUMNS } from './DealTable';
import { num } from './format';
import { Async, Fresh, Kpi, Section } from './parts';
import type { Route } from './route';
import type { TodayResponse } from './types';
import { useApi } from './useApi';

const FILTER_LABELS = { today: '今日動く候補', soon: '今週満了', started: '今週始まった契約', before: '開始前の契約' };
type TodayFilter = keyof typeof FILTER_LABELS | '';

export function TodayView({ route, go }: { route: Route; go: (r: Partial<Route>) => void }) {
  const [selected, setSelected] = useState<TodayFilter>('');
  const toggle = (key: Exclude<TodayFilter, ''>): void => { setSelected((v) => v === key ? '' : key); };
  const [reload, setReload] = useState(0);
  const result = useApi<TodayResponse>('/api/consulting/today', reload);
  return (
    <Async result={result} onRetry={() => { setReload((n) => n + 1); }}>
      {(d) => {
        const only = { c: route.c, flag: '', band: '' };
        const source = selected === 'soon' ? d.expiring_this_week ?? [] : selected === 'started' ? d.started_this_week ?? [] : selected === 'before' ? d.not_started ?? [] : route.c !== '' ? d.candidates ?? d.rows : d.rows.concat(d.extra_low_satisfaction ?? []);
        const rows = filterRows(source, only);
        const soon = filterRows(d.expiring_this_week ?? [], only);
        const started = filterRows(d.started_this_week ?? [], only);
        const open = (id: string): void => { go({ view: 'detail', id }); };
        return (
          <>
            <Fresh meta={d.meta} />
            {route.c !== '' && <p className="cv2-dim">札の件数は全社です。一覧は選んだ担当で絞っています。</p>}
            <div className="cv2-kpis">
              <Kpi active={selected === 'today'} onClick={() => { toggle('today'); }} label="今日動く候補" value={num(d.meta.n_hit)} unit="件" note={`注意が2つ以上 ${num(d.meta.n_flags2)} 件を含む`} />
              <Kpi active={selected === 'soon'} onClick={() => { toggle('soon'); }} label="今週満了" value={num((d.expiring_this_week ?? []).length)} unit="件" note="満了まで7日以内" />
              <Kpi active={selected === 'started'} onClick={() => { toggle('started'); }} label="今週始まった契約" value={num((d.started_this_week ?? []).length)} unit="件" note="契約開始日が直近の案件" />
              <Kpi active={selected === 'before'} onClick={() => { toggle('before'); }} label="開始前の契約" value={num((d.not_started ?? []).length)} unit="件" note="開始日がまだ先の案件" />
            </div>
            <div className="cv2-filters">
              <ConsultantSelect value={route.c} names={d.consultants ?? []} onChange={(v) => { go({ c: v }); }} />
            </div>
            {selected !== '' && <p className="cv2-note" role="status">絞り込み中: {FILTER_LABELS[selected]} <button type="button" onClick={() => { setSelected(''); }}>解除</button></p>}
            <Section title={`${selected === '' || selected === 'today' ? '今日動く先' : FILTER_LABELS[selected]}（${num(rows.length)} 件）`}>
              {(selected === '' || selected === 'today') && d.meta.filter_rule ? <p className="cv2-dim">{d.meta.filter_rule}</p> : null}
              <DealTable rows={rows} columns={TODAY_COLUMNS} onOpen={open} caption="今日動く案件" portal={d.meta.hubspot_portal_id} />
            </Section>
            <Section title={`今週満了する案件（${num(soon.length)} 件）`}>
              <DealTable rows={soon} columns={TODAY_COLUMNS} onOpen={open} caption="今週満了する案件" portal={d.meta.hubspot_portal_id} />
            </Section>
            <details className="cv2-more"><summary>今週始まった契約（{num(started.length)} 件）</summary>
            <Section title={`今週始まった契約（${num(started.length)} 件）`}>
              <DealTable rows={started} columns={TODAY_COLUMNS} onOpen={open} caption="今週始まった契約" portal={d.meta.hubspot_portal_id} />
            </Section></details>
            <StageStay consultant={route.c} go={go} />
          </>
        );
      }}
    </Async>
  );
}
