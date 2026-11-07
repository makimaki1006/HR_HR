import { Overdue } from './SupplementalViews';
import { useState } from 'react';
import { DASH, num, orText, yen } from './format';
import { Async, CaseLink, FlagChips, Fresh, HubspotLink, Kpi, Section } from './parts';
import type { Route } from './route';
import type { PipeAmount, PipeRow, RenewalPipeResponse } from './types';
import { useApi } from './useApi';

const WORDS = ['今月', '来月', '再来月'];

function amountText(s: PipeAmount): string {
  return `${s.amount === null ? '金額なし' : yen(s.amount)}（金額が空 ${num(s.amount_missing)} 件）`;
}

function PipeTable({ rows, portal, go }: { rows: PipeRow[]; portal: string | undefined; go: (r: Partial<Route>) => void }) {
  if (rows.length === 0) return <p className="cv2-empty">該当する契約はありません。</p>;
  return (
    <div className="cv2-table-wrap">
      <table className="cv2-table">
        <thead>
          <tr>
            <th scope="col">満了日</th>
            <th scope="col" className="cv2-num">満了まで</th>
            <th scope="col">案件</th>
            <th scope="col">担当</th>
            <th scope="col">ステージ</th>
            <th scope="col" className="cv2-num">金額</th>
            <th scope="col">注意</th>
            <th scope="col">リンク</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.deal_id}>
              <td>{r.expiry ?? DASH}</td>
              <td className="cv2-num">
                {r.days_left === null || r.days_left === undefined ? DASH : r.days_left < 0 ? <b className="cv2-red">満了を{num(-r.days_left)}日過ぎている</b> : r.days_left <= 60 ? <b className="cv2-amber">{num(r.days_left)}日</b> : `${num(r.days_left)}日`}
              </td>
              <td>
                <button type="button" className="cv2-link" onClick={() => { go({ view: 'detail', id: r.deal_id }); }}>
                  {orText(r.name, '取引名なし')}
                </button>
              </td>
              <td>{orText(r.consultant, DASH)}</td>
              <td>{r.stage === null || r.stage === undefined || r.stage === '' ? <small>ステージ名なし</small> : r.stage}</td>
              <td className="cv2-num">{yen(r.amount)}</td>
              <td><FlagChips flags={r.flags} /></td>
              <td><CaseLink dealId={r.deal_id} /> <HubspotLink portal={portal} dealId={r.deal_id} /></td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function RenewalView({ go }: { go: (r: Partial<Route>) => void }) {
  const [reload, setReload] = useState(0);
  const result = useApi<RenewalPipeResponse>('/api/consulting/renewal-pipe', reload);
  return (
    <Async result={result} onRetry={() => { setReload((n) => n + 1); }}>
      {(d) => {
        const win = d.meta.window ?? [];
        const months = d.months ?? [];
        const rows = d.rows ?? [];
        const ov = d.overdue_before;
        const wt = d.window_total;
        const at = d.active_total;
        const portal = d.meta.hubspot_portal_id;
        const stages = d.stages ?? [];
        const cnt = (m: (typeof months)[number], s: string): number => m.stages?.find((x) => x.label === s)?.n ?? 0;
        return (
          <>
            <Fresh meta={d.meta} />
            <div className="cv2-kpis">
              {months.map((m, i) => (
                <Kpi key={m.month} label={`${WORDS[i] ?? ''} ${win[i] ?? ''} に満了`} value={num(m.n)} unit="件" note={amountText(m)} />
              ))}
              {ov && ov.sum.n > 0 ? <Kpi label="先月以前に満了日を過ぎて、まだ稼働中" value={num(ov.sum.n)} unit="件" note={amountText(ov.sum)} /> : null}
            </div>
            <p className="cv2-note">
              稼働中 {num(d.meta.n_active)} 件（オプション契約を除く）のうち、今月〜再来月に満了 {num(wt?.n ?? 0)} 件・先月以前に満了日を過ぎてまだ稼働中 {num(ov?.sum.n ?? 0)} 件・再来月より先に満了 {num(d.later ?? 0)} 件・満了日が入っていない {num(d.no_expiry ?? 0)} 件です（どの契約もどれか 1 つにだけ入ります）。
              今月〜再来月の金額の合計は {wt ? amountText(wt) : DASH}、稼働中の全部では {at ? amountText(at) : DASH}。{d.meta.amount_basis ?? ''}。
            </p>
            {win.map((w, i) => {
              const rs = rows.filter((r) => (r.expiry ?? '').slice(0, 7) === w);
              return (
                <Section key={w} title={`${WORDS[i] ?? ''} ${w} に満了する契約（満了の近い順、${num(rs.length)} 件）`}>
                  <PipeTable rows={rs} portal={portal} go={go} />
                </Section>
              );
            })}
            {ov && ov.sum.n > 0 && (
              <Section title={`先月以前に満了日を過ぎて、まだ稼働中の契約（${num(ov.sum.n)} 件）`}>
                <details className="cv2-more">
                  <summary>表を開く（満了の古い順）</summary>
                  <PipeTable rows={ov.rows} portal={portal} go={go} />
                </details>
              </Section>
            )}
            <Overdue go={go} />
            <Section title="ステージ別の件数（満了月ごと・実数）">
              <div className="cv2-table-wrap">
                <table className="cv2-table">
                  <thead>
                    <tr>
                      <th scope="col">ステージ</th>
                      {months.map((m, i) => (
                        <th key={m.month} scope="col" className="cv2-num">{WORDS[i] ?? ''} {win[i] ?? ''}</th>
                      ))}
                      <th scope="col" className="cv2-num">3か月の計</th>
                    </tr>
                  </thead>
                  <tbody>
                    {stages.map((s) => (
                      <tr key={s}>
                        <td>{s}</td>
                        {months.map((m) => <td key={m.month} className="cv2-num">{num(cnt(m, s))}</td>)}
                        <td className="cv2-num">{num(months.reduce((a, m) => a + cnt(m, s), 0))}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <p className="cv2-dim">並びは HubSpot のステージの並び（工程の順）です。件数だけを数えています。ステージ名の「50％」などは名前の一部で、件数にも金額にも掛けていません。</p>
            </Section>
          </>
        );
      }}
    </Async>
  );
}
