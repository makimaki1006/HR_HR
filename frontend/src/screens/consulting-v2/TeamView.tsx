import { DASH, num, pct1 } from './format';
import { ContactTrend } from './ContactTrend';
import { Async, Kpi, Section } from './parts';
import type { Route } from './route';
import type { ContactCell, ContactPeriod, TeamResponse, TeamStatusRow } from './types';
import { useApi } from './useApi';
import { useState } from 'react';

function lastFixed(periods: ContactPeriod[] | undefined): number {
  const list = periods ?? [];
  for (let i = list.length - 1; i >= 0; i--) {
    const p = list[i];
    if (p && p.provisional !== true && p.calls_missing !== true) return i;
  }
  return -1;
}

function Delta({ v }: { v: number | undefined }) {
  if (v === undefined || v === 0) return null;
  return <span className={`cv2-delta ${v > 0 ? 'cv2-delta-worse' : 'cv2-delta-better'}`}>{v > 0 ? '+' : '−'}{num(Math.abs(v))}</span>;
}

interface Merged extends TeamStatusRow {
  contact_rate: number | null;
  contact_touched: number | null;
  contact_months: number | null;
  small_n: boolean;
  focus: number | null;
  cell: ContactCell | null;
}

function merge(d: TeamResponse): { rows: Merged[]; monthLabel: string } {
  const cons = new Map((d.consultants?.rows ?? []).map((r) => [r.consultant, r]));
  const month = d.contact?.month;
  const lf = lastFixed(month?.periods);
  const cells = new Map((month?.rows ?? []).map((r) => [r.consultant, r.cells]));
  const rows = d.status.rows.map((s): Merged => {
    const c = cons.get(s.consultant);
    return {
      ...s,
      contact_rate: c?.contact_rate ?? null,
      contact_touched: c?.contact_touched ?? null,
      contact_months: c?.contact_months ?? null,
      small_n: c?.small_n === true,
      focus: c?.focus ?? null,
      cell: lf >= 0 ? (cells.get(s.consultant)?.[lf] ?? null) : null,
    };
  });
  return { rows, monthLabel: lf >= 0 ? (month?.periods?.[lf]?.label ?? '') : '' };
}

type SortKey = 'consultant' | 'n_active' | 'unattended' | 'n_flags2' | 'mtg_critical' | 'no_contact' | 'expiring60' | 'nps_low' | 'expiring_week' | 'contact_rate';

export function TeamView({ go }: { go: (r: Partial<Route>) => void }) {
  const [reload, setReload] = useState(0);
  const result = useApi<TeamResponse>('/api/consulting/team', reload);
  const [sort, setSort] = useState<{ key: SortKey; asc: boolean }>({ key: 'n_flags2', asc: false });
  return (
    <Async result={result} onRetry={() => { setReload((n) => n + 1); }}>
      {(d) => {
        const { rows, monthLabel } = merge(d);
        const sm = d.status.meta;
        const labels = sm.flag_labels ?? {};
        const sorted = rows.slice().sort((a, b) => {
          const x = a[sort.key];
          const y = b[sort.key];
          if (x === null || x === undefined) return y === null || y === undefined ? 0 : 1;
          if (y === null || y === undefined) return -1;
          const c = typeof x === 'string' && typeof y === 'string' ? x.localeCompare(y, 'ja') : Number(x) - Number(y);
          return (sort.asc ? c : -c) || a.consultant.localeCompare(b.consultant, 'ja');
        });
        const deltas = d.comparison?.totals ?? {};
        const total = (k: keyof TeamStatusRow): number =>
          rows.reduce((s, r) => s + (Number(r[k]) || 0), sm.unknown?.[k as string] ?? 0);
        const drill = (n: number, to: Partial<Route>, what: string) =>
          n > 0 ? (
            <button type="button" className="cv2-drill" title={what} onClick={() => { go(to); }}>
              {num(n)}
            </button>
          ) : (
            <>{num(n)}</>
          );
        const th = (key: SortKey, label: string, numeric = true) => (
          <th scope="col" className={numeric ? 'cv2-num' : undefined} aria-sort={sort.key === key ? (sort.asc ? 'ascending' : 'descending') : 'none'}>
            <button type="button" className="cv2-sort" onClick={() => { setSort({ key, asc: sort.key === key ? !sort.asc : key === 'consultant' }); }}>
              {label}
              {sort.key === key ? (sort.asc ? ' ▲' : ' ▼') : ''}
            </button>
          </th>
        );
        return (
          <>
            {d.comparison?.data_day ? (
              <p className="cv2-dim">
                {Number(d.comparison.data_day.split('-')[1])}/{Number(d.comparison.data_day.split('-')[2])} のデータとの差
              </p>
            ) : null}
            <div className="cv2-kpis">
              <Kpi
                label="担当者"
                value={num(d.meta.n_consultant)}
                unit="名"
                note={d.meta.unknown_owner ? `担当不明の稼働中 ${num(d.meta.unknown_owner)} 件は表から除外` : '稼働中の案件を持つ人'}
              />
              <Kpi
                label={`注意が ${num(sm.min_flags)} つ以上の案件`}
                value={num(sm.n_flags2)}
                unit="件"
                note={`稼働中 ${num(sm.n_active)} 件のうち`}
                onClick={() => { go({ view: 'today' }); }}
              />
              <Kpi label="今週満了" value={num(sm.expiring_week)} unit="件" note={`満了まで ${num(sm.week_days)} 日以内`} />
              <Kpi
                label="退職者のまま"
                value={num(d.meta.retired_deals)}
                unit="件"
                note={`稼働中の案件（退職者 ${num(d.meta.retired_people)} 名）`}
              />
            </div>
            <Section title={`担当者ごとの状態（${num(rows.length)} 名）`}>
              <p className="cv2-dim">
                持ち件数は稼働中の案件数です。{monthLabel ? `接触回数は確定した最新月（${monthLabel}）の値です。` : ''}
                数字を押すと、その担当の対象案件が開きます。
              </p>
              <div className="cv2-table-wrap">
                <table className="cv2-table" aria-label="担当者ごとの状態">
                  <thead>
                    <tr>
                      {th('consultant', '担当者', false)}
                      {th('n_active', '持ち件数')}
                      {th('unattended', '手が回っていない案件')}
                      {th('n_flags2', '注意2つ以上')}
                      {th('mtg_critical', 'MTG 60日以上空いている')}
                      {th('no_contact', '接触の記録が無い')}
                      {th('expiring60', '満了まで60日以内')}
                      {th('nps_low', '満足度 4以下')}
                      {th('expiring_week', '今週満了')}
                      {th('contact_rate', '接触した月の割合')}
                      <th scope="col" className="cv2-num">1件あたりの接触回数</th>
                      <th scope="col" className="cv2-num">重点顧客の案件</th>
                    </tr>
                  </thead>
                  <tbody>
                    {sorted.map((r) => (
                      <tr key={r.consultant}>
                        <td>
                          <button type="button" className="cv2-link" onClick={() => { go({ view: 'board', c: r.consultant }); }}>
                            {r.consultant}
                          </button>
                          {r.retired === true ? <span className="cv2-tag cv2-tag-red">退職者のまま</span> : null}
                        </td>
                        <td className="cv2-num">{num(r.n_active)}<Delta v={r.delta?.n_active} /></td>
                        <td className="cv2-num">{num(r.unattended)}</td>
                        <td className="cv2-num">
                          {drill(r.n_flags2, { view: 'today', c: r.consultant }, 'この担当で「今日」を開く')}
                          <Delta v={r.delta?.n_flags2} />
                        </td>
                        <td className="cv2-num">
                          {drill(r.mtg_critical, { view: 'board', c: r.consultant, band: 'critical' }, 'この担当のMTGが空いている案件を開く')}
                          <Delta v={r.delta?.mtg_critical} />
                        </td>
                        <td className="cv2-num">
                          {drill(r.no_contact, { view: 'board', c: r.consultant, flag: labels.no_contact ?? '' }, 'この担当の接触の記録が無い案件を開く')}
                          <Delta v={r.delta?.no_contact} />
                        </td>
                        <td className="cv2-num">
                          {drill(r.expiring60, { view: 'board', c: r.consultant, flag: labels.expiring60 ?? '' }, 'この担当の満了が近い案件を開く')}
                          <Delta v={r.delta?.expiring60} />
                        </td>
                        <td className="cv2-num">
                          {drill(r.nps_low, { view: 'board', c: r.consultant, flag: labels.nps_low ?? '' }, 'この担当の満足度が低い案件を開く')}
                          <Delta v={r.delta?.nps_low} />
                        </td>
                        <td className="cv2-num">{num(r.expiring_week)}<Delta v={r.delta?.expiring_week} /></td>
                        <td className="cv2-num">
                          {r.contact_rate === null ? DASH : (
                            <>
                              <b className={r.contact_rate < 40 ? 'cv2-red' : r.contact_rate < 70 ? 'cv2-amber' : 'cv2-green'}>
                                {r.contact_rate < 40 ? '▼ ' : ''}{pct1(r.contact_rate)}
                              </b>
                              <small> {num(r.contact_touched)}/{num(r.contact_months)} か月</small>
                              {r.small_n ? <span className="cv2-tag" title="分母（案件 × 経過月）が小さく、たまたまで大きく振れます">対象が小さい</span> : null}
                            </>
                          )}
                        </td>
                        <td className="cv2-num">
                          {r.cell?.deals ? (
                            <>
                              {num(r.cell.avg ?? null, 2)}
                              <small> {num(r.cell.contacts ?? null)}/{num(r.cell.deals)}件</small>
                              {r.cell.small_n === true ? <span className="cv2-tag">少</span> : null}
                            </>
                          ) : DASH}
                        </td>
                        <td className="cv2-num">{r.focus === null ? DASH : num(r.focus)}</td>
                      </tr>
                    ))}
                  </tbody>
                  <tfoot>
                    <tr>
                      <th scope="row">全体</th>
                      <td className="cv2-num">{sm.n_active === undefined ? DASH : num(sm.n_active)}<Delta v={deltas.n_active} /></td>
                      <td className="cv2-num">{num(total('unattended'))}</td>
                      <td className="cv2-num">{num(total('n_flags2'))}<Delta v={deltas.n_flags2} /></td>
                      <td className="cv2-num">{num(total('mtg_critical'))}<Delta v={deltas.mtg_critical} /></td>
                      <td className="cv2-num">{num(total('no_contact'))}<Delta v={deltas.no_contact} /></td>
                      <td className="cv2-num">{num(total('expiring60'))}<Delta v={deltas.expiring60} /></td>
                      <td className="cv2-num">{num(total('nps_low'))}<Delta v={deltas.nps_low} /></td>
                      <td className="cv2-num">{num(total('expiring_week'))}<Delta v={deltas.expiring_week} /></td>
                      <td className="cv2-num">{DASH}</td>
                      <td className="cv2-num">{DASH}</td>
                      <td className="cv2-num">{DASH}</td>
                    </tr>
                  </tfoot>
                </table>
              </div>
              {sm.unknown?.n_active ? (
                <p className="cv2-dim">
                  担当が取れない稼働中の案件 {num(sm.unknown.n_active)} 件（うち注意2つ以上 {num(sm.unknown.n_flags2)} 件）は表に入れていません。
                </p>
              ) : null}
              <p className="cv2-dim">
                「接触した月の割合」は、経過した案件月のうち接触（MTG または60秒超の電話）があった月の割合です。
                「1件あたりの接触回数」は、確定した最新月の接触回数 ÷ 持ち件数で、別の指標です。担当者の評価ではありません。
              </p>
            </Section>
            {d.contact ? <ContactTrend contact={d.contact} /> : null}
          </>
        );
      }}
    </Async>
  );
}
