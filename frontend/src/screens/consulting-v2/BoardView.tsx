import { useState } from 'react';
import { BOARD_COLUMNS, DealTable } from './DealTable';
import { num } from './format';
import { Async, Kpi, Section } from './parts';
import type { Route } from './route';
import type { BoardResponse, DealRow } from './types';
import { useApi } from './useApi';

export function filterRows(rows: DealRow[], r: Pick<Route, 'c' | 'flag' | 'band'>): DealRow[] {
  return rows.filter((x) => {
    if (r.c !== '' && (x.consultant ?? '') !== r.c) return false;
    if (r.flag !== '' && !(x.flags ?? []).includes(r.flag)) return false;
    if (r.band !== '' && (x.mtg_band ?? '') !== r.band) return false;
    return true;
  });
}

export function ConsultantSelect({
  value,
  names,
  onChange,
}: {
  value: string;
  names: string[];
  onChange: (v: string) => void;
}) {
  return (
    <label className="cv2-field">
      コンサル担当者
      <select value={value} onChange={(e) => { onChange(e.target.value); }}>
        <option value="">すべて</option>
        {names.map((n) => (
          <option key={n} value={n}>{n}</option>
        ))}
      </select>
    </label>
  );
}

export function BoardView({ route, go }: { route: Route; go: (r: Partial<Route>) => void }) {
  const [reload, setReload] = useState(0);
  const result = useApi<BoardResponse>('/api/consulting/deals', reload);
  return (
    <Async result={result} onRetry={() => { setReload((n) => n + 1); }}>
      {(d) => {
        const names = [...new Set(d.rows.map((r) => r.consultant ?? '').filter((n) => n !== ''))].sort((a, b) => a.localeCompare(b, 'ja'));
        const shown = filterRows(d.rows, route);
        const flagOptions = (d.meta.flag_counts ?? []).slice().sort((a, b) => b.n - a.n);
        const bandLabel = d.meta.mtg_gap?.bands?.find((b) => b.band === route.band)?.label;
        const narrowed = route.c !== '' || route.flag !== '' || route.band !== '';
        return (
          <>
            <div className="cv2-kpis">
              <Kpi label="稼働中の案件" value={num(d.meta.n_active ?? d.rows.length)} unit="件" note="注意の数が多い順（同じなら金額の大きい順）" />
              {flagOptions.slice(0, 3).map((f) => (
                <Kpi
                  key={f.label}
                  label={f.label}
                  value={num(f.n)}
                  unit="件"
                  note="押すと、この注意で絞ります"
                  onClick={() => { go({ flag: f.label, band: '' }); }}
                />
              ))}
            </div>
            <div className="cv2-filters">
              <ConsultantSelect value={route.c} names={names} onChange={(v) => { go({ c: v }); }} />
              <label className="cv2-field">
                注意
                <select value={route.flag} onChange={(e) => { go({ flag: e.target.value }); }}>
                  <option value="">すべて</option>
                  {flagOptions.map((f) => (
                    <option key={f.label} value={f.label}>{f.label}（{num(f.n)}）</option>
                  ))}
                </select>
              </label>
              {narrowed && (
                <button type="button" onClick={() => { go({ c: '', flag: '', band: '' }); }}>
                  条件を解除
                </button>
              )}
            </div>
            <Section title="案件一覧">
              <p className="cv2-count" aria-live="polite">
                <b>{num(d.rows.length)} 件中 {num(shown.length)} 件</b>を表示
                {narrowed
                  ? `（${[route.c !== '' ? `担当 ${route.c}` : '', route.flag !== '' ? `注意 ${route.flag}` : '', route.band !== '' ? `MTGが空いている期間 ${bandLabel ?? '区分'}` : ''].filter((x) => x !== '').join('・')}）`
                  : '（全部出しています）'}
              </p>
              <DealTable rows={shown} columns={BOARD_COLUMNS} onOpen={(id) => { go({ view: 'detail', id }); }} caption="稼働中の案件一覧" />
            </Section>
          </>
        );
      }}
    </Async>
  );
}
