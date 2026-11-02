import { useState } from 'react';
import { ConsultantSelect, filterRows } from './BoardView';
import { DealTable, TODAY_COLUMNS } from './DealTable';
import { num } from './format';
import { Async, Kpi, Section } from './parts';
import type { Route } from './route';
import type { TodayResponse } from './types';
import { useApi } from './useApi';

export function TodayView({ route, go }: { route: Route; go: (r: Partial<Route>) => void }) {
  const [reload, setReload] = useState(0);
  const result = useApi<TodayResponse>('/api/consulting/today', reload);
  return (
    <Async result={result} onRetry={() => { setReload((n) => n + 1); }}>
      {(d) => {
        const only = { c: route.c, flag: '', band: '' };
        const rows = filterRows(d.rows, only);
        const soon = filterRows(d.expiring_this_week ?? [], only);
        const started = filterRows(d.started_this_week ?? [], only);
        const open = (id: string): void => { go({ view: 'detail', id }); };
        return (
          <>
            <div className="cv2-kpis">
              <Kpi label="今日動く候補" value={num(d.meta.n_hit)} unit="件" note={`注意が2つ以上 ${num(d.meta.n_flags2)} 件を含む`} />
              <Kpi label="今週満了" value={num((d.expiring_this_week ?? []).length)} unit="件" note="満了まで7日以内" />
              <Kpi label="今週始まった契約" value={num((d.started_this_week ?? []).length)} unit="件" note="契約開始日が直近の案件" />
              <Kpi label="開始前の契約" value={num((d.not_started ?? []).length)} unit="件" note="開始日がまだ先の案件" />
            </div>
            <div className="cv2-filters">
              <ConsultantSelect value={route.c} names={d.consultants ?? []} onChange={(v) => { go({ c: v }); }} />
            </div>
            <Section title={`今日動く先（${num(rows.length)} 件）`}>
              {d.meta.filter_rule ? <p className="cv2-dim">{d.meta.filter_rule}</p> : null}
              <DealTable rows={rows} columns={TODAY_COLUMNS} onOpen={open} caption="今日動く案件" />
            </Section>
            <Section title={`今週満了する案件（${num(soon.length)} 件）`}>
              <DealTable rows={soon} columns={TODAY_COLUMNS} onOpen={open} caption="今週満了する案件" />
            </Section>
            <Section title={`今週始まった契約（${num(started.length)} 件）`}>
              <DealTable rows={started} columns={TODAY_COLUMNS} onOpen={open} caption="今週始まった契約" />
            </Section>
          </>
        );
      }}
    </Async>
  );
}
