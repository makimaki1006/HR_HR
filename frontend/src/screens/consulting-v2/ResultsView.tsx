import { useState } from 'react';
import { DASH, num, orText, pct1, yen } from './format';
import { Async, Fresh, Kpi, Section } from './parts';
import type { Route } from './route';
import type { MtgQualityResponse, PhoneResponse, ResultsResponse } from './types';
import { useApi } from './useApi';

import { DataQuality, ResultsDetails } from './ResultsDetails';

const MIN_N = 5;

function Rate({ v }: { v: number | null | undefined }) {
  return <>{v === null || v === undefined ? DASH : pct1(v)}</>;
}

function Mtg({ result, retry }: { result: ReturnType<typeof useApi<MtgQualityResponse>>; retry: () => void }) {
  return (
    <Section title="MTG の品質">
      <Async result={result} onRetry={retry}>
        {(m) => (
          <>
            <p>
              取引に紐づいた MTG <b>{num(m.linked?.n ?? 0)}</b> 件 / {num(m.meta.n_mtg ?? 0)} 件（<Rate v={m.linked?.rate} />）
            </p>
            {m.linked?.note ? <p className="cv2-dim">{m.linked.note}</p> : null}
            <div className="cv2-table-wrap">
              <table className="cv2-table">
                <thead>
                  <tr><th scope="col">項目</th><th scope="col" className="cv2-num">埋まっている件数</th><th scope="col" className="cv2-num">割合</th></tr>
                </thead>
                <tbody>
                  {(m.filled ?? []).map((f) => (
                    <tr key={f.field}><td>{f.field}</td><td className="cv2-num">{num(f.n)}</td><td className="cv2-num"><Rate v={f.rate} /></td></tr>
                  ))}
                </tbody>
              </table>
            </div>
            {m.filled_note ? <p className="cv2-dim">{m.filled_note}</p> : null}
            {(m.risk_dist ?? []).length > 0 && (
              <p className="cv2-dim">リスク判定の内訳: {(m.risk_dist ?? []).map((r) => `${orText(r.label, '判定なし')} ${num(r.n)} 件`).join('・')}</p>
            )}
          </>
        )}
      </Async>
    </Section>
  );
}

function Phone({ result, retry, go }: { result: ReturnType<typeof useApi<PhoneResponse>>; retry: () => void; go: (r: Partial<Route>) => void }) {
  return (
    <Section title="電話の到達">
      <Async result={result} onRetry={retry}>
        {(p) => (
          <>
            <div className="cv2-kpis">
              <Kpi label="通話の記録が1本も無い稼働中の契約" value={num(p.reach?.no_call ?? 0)} unit="件" note={p.reach?.no_call_rate === null || p.reach?.no_call_rate === undefined ? undefined : `稼働中の ${pct1(p.reach.no_call_rate)}`} />
              <Kpi label={`${num(p.meta.threshold_sec ?? 60)}秒超の接触が無い稼働中の契約`} value={num(p.reach?.no_contact ?? 0)} unit="件" note={p.reach?.no_contact_rate === null || p.reach?.no_contact_rate === undefined ? undefined : `稼働中の ${pct1(p.reach.no_contact_rate)}`} />
            </div>
            {p.reach?.note ? <p className="cv2-dim">{p.reach.note}</p> : null}
            {(p.monthly ?? []).length > 0 && (
              <div className="cv2-table-wrap">
                <table className="cv2-table">
                  <thead><tr><th scope="col">月</th><th scope="col" className="cv2-num">通話</th><th scope="col" className="cv2-num">接触</th><th scope="col" className="cv2-num">接触した割合</th></tr></thead>
                  <tbody>
                    {(p.monthly ?? []).map((r) => (
                      <tr key={r.month}><td>{r.month}</td><td className="cv2-num">{num(r.calls)}</td><td className="cv2-num">{num(r.contacts)}</td><td className="cv2-num"><Rate v={r.contact_rate} /></td></tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
            {p.silent && p.silent.n > 0 && (
              <details className="cv2-more">
                <summary>接触が途絶えている契約（{num(p.silent.n)} 件）{p.silent.rule ? `: ${p.silent.rule}` : ''}</summary>
                <ul className="cv2-results">
                  {(p.silent.rows ?? []).slice(0, 50).map((r) => (
                    <li key={r.deal_id}>
                      <button type="button" className="cv2-link" onClick={() => { go({ view: 'detail', id: r.deal_id }); }}>{orText(r.name, '取引名なし')}</button>
                      <small> {r.stage ?? ''} ／ 通話 {num(r.n_calls ?? 0)} 本・接触 {num(r.n_contact ?? 0)} 本 ／ 最後の接触 {r.last_contact ?? '記録なし'}</small>
                    </li>
                  ))}
                </ul>
              </details>
            )}
          </>
        )}
      </Async>
    </Section>
  );
}

export function ResultsView({ go }: { go: (r: Partial<Route>) => void }) {
  const [reload, setReload] = useState(0);
  const result = useApi<ResultsResponse>('/api/consulting/results', reload);
  const mtg = useApi<MtgQualityResponse>('/api/consulting/mtg-quality', reload);
  const phone = useApi<PhoneResponse>('/api/consulting/phone', reload);
  const retry = (): void => { setReload((n) => n + 1); };
  return (
    <>
      <Async result={result} onRetry={retry}>
        {(d) => {
          const mr = d.renewal?.monthly_retention?.rows ?? [];
          let last: (typeof mr)[number] | null = null;
          for (const r of mr) if (r.denom >= MIN_N && !(r.pending > 0) && r.rate !== null) last = r;
          const money = d.money;
          const moneyRows = money?.retention?.rows ?? [];
          const lastMoney = last ? (moneyRows.find((r) => r.month === last.month) ?? null) : null;
          const ga = d.outcome?.goal_act;
          const reached = ga?.bands?.find((b) => b.label.startsWith('100%'))?.n;
          return (
            <>
              <Fresh meta={d.meta} />
              <div className="cv2-kpis">
                <Kpi label="稼働中の契約金額" value={money?.active_total ? yen(money.active_total.amount) : DASH} note={money?.active_total ? `対象 ${num(money.active_total.n)} 件・金額が空 ${num(money.active_total.amount_missing)} 件` : undefined} />
                <Kpi
                  label={last ? `月次の継続率（${last.month} に満了）` : '月次の継続率'}
                  value={last ? pct1(last.rate) : DASH}
                  note={last ? `結果が出た ${num(last.denom)} 件のうち（継続 ${num(last.keep)}・解約 ${num(last.cancel)}・充足 ${num(last.fill)}）` : `結果がそろい対象が ${String(MIN_N)} 件以上の満了月がありません`}
                />
                <Kpi
                  label="金額で見た継続率"
                  value={lastMoney ? pct1(lastMoney.rate) : DASH}
                  note={lastMoney ? `${lastMoney.month} に満了。継続 ${yen(lastMoney.keep)} / 母数 ${yen(lastMoney.denom)}` : '件数の継続率と同じ満了月が出せません'}
                />
                <Kpi label="採用目標に届いた稼働中の契約" value={reached === undefined ? DASH : num(reached)} unit={reached === undefined ? undefined : '件'} note={ga?.both ? `目標・承諾数あり ${num(ga.both)} 件（途中）` : '目標が入っている稼働中の契約がありません'} />
              </div>
              <Section title="月ごとの継続率（満了月ごと）">
                <div className="cv2-table-wrap">
                  <table className="cv2-table">
                    <thead>
                      <tr>
                        <th scope="col">満了月</th>
                        <th scope="col" className="cv2-num">継続</th>
                        <th scope="col" className="cv2-num">解約</th>
                        <th scope="col" className="cv2-num">充足</th>
                        <th scope="col" className="cv2-num">結果待ち</th>
                        <th scope="col" className="cv2-num">継続率</th>
                        <th scope="col" className="cv2-num">充足を除く継続率</th>
                      </tr>
                    </thead>
                    <tbody>
                      {mr.map((r) => (
                        <tr key={r.month}>
                          <td>{r.month}</td>
                          <td className="cv2-num">{num(r.keep)}</td>
                          <td className="cv2-num">{num(r.cancel)}</td>
                          <td className="cv2-num">{num(r.fill)}</td>
                          <td className="cv2-num">{num(r.pending)}</td>
                          <td className="cv2-num"><Rate v={r.rate} />{r.denom < MIN_N ? <small> 対象が小さい</small> : null}</td>
                          <td className="cv2-num"><Rate v={r.rate_excl_fill} /></td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
                <p className="cv2-dim">充足は採れて終わった契約で、継続率の分母に入れています。結果待ちの月は率を信用しないでください（結果が確定していません）。金額は契約期間全体の額で、月額ではありません。</p>
              </Section>
              <Section title="継続回数ごとの解約率">
                <div className="cv2-table-wrap">
                  <table className="cv2-table">
                    <thead>
                      <tr>
                        <th scope="col">継続回数</th>
                        <th scope="col" className="cv2-num">契約数</th>
                        <th scope="col" className="cv2-num">稼働中</th>
                        <th scope="col" className="cv2-num">継続</th>
                        <th scope="col" className="cv2-num">解約</th>
                        <th scope="col" className="cv2-num">充足</th>
                        <th scope="col" className="cv2-num">解約率（充足を含む）</th>
                        <th scope="col" className="cv2-num">解約率（充足を除く）</th>
                      </tr>
                    </thead>
                    <tbody>
                      {(d.renewal?.by_renewal ?? []).map((r) => (
                        <tr key={r.renewal_no}>
                          <td>{r.renewal_no === 0 ? '継続0回' : `${num(r.renewal_no)}回目の継続`}</td>
                          <td className="cv2-num">{num(r.n)}</td>
                          <td className="cv2-num">{num(r.n_active)}</td>
                          <td className="cv2-num">{num(r.keep)}</td>
                          <td className="cv2-num">{num(r.cancel)}</td>
                          <td className="cv2-num">{num(r.fill)}</td>
                          <td className="cv2-num"><Rate v={r.cancel_rate} /></td>
                          <td className="cv2-num"><Rate v={r.cancel_rate_excl_fill} /></td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </Section>
              {ga && (
                <Section title="採用目標に対する進捗（稼働中の契約）">
                  <p className="cv2-dim">目標が入っているのは稼働中 {num(ga.has_goal ?? 0)} / {num(ga.pop ?? 0)} 件。途中の値で、届いていない契約はまだ伸びます。</p>
                  <ul className="cv2-results">
                    {(ga.bands ?? []).map((b) => (
                      <li key={b.label}>{b.label}: <b>{num(b.n)}</b> 件</li>
                    ))}
                  </ul>
                </Section>
              )}
              <ResultsDetails d={d} go={go} />
              {d.headquarters && (
                <Section title="本部向け（法人ごとの拠点と契約）">
                  <p className="cv2-dim">{d.headquarters.meta?.cancel_rule ?? ''}</p>
                  <div className="cv2-table-wrap">
                    <table className="cv2-table">
                      <thead><tr><th scope="col">法人</th><th scope="col" className="cv2-num">拠点数</th><th scope="col" className="cv2-num">契約数</th><th scope="col" className="cv2-num">稼働中</th></tr></thead>
                      <tbody>
                        {(d.headquarters.rows ?? []).slice(0, 50).map((r) => (
                          <tr key={r.houjin}>
                            <td><button type="button" className="cv2-link" onClick={() => { go({ view: 'customer', houjin: r.houjin }); }}>{orText(r.name, '法人名なし')}</button></td>
                            <td className="cv2-num">{num(r.sites)}</td>
                            <td className="cv2-num">{num(r.deals)}</td>
                            <td className="cv2-num">{num(r.active)}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </Section>
              )}
              <Section title="担当の交代">
                <p className="cv2-dim">
                  {num(d.handover?.meta?.n ?? 0)} 件{d.handover?.meta?.median_gap_days === null || d.handover?.meta?.median_gap_days === undefined ? '' : `（担当欄の書き換えとのずれの中央値 ${num(d.handover.meta.median_gap_days)} 日）`}
                </p>
                <div className="cv2-table-wrap">
                  <table className="cv2-table">
                    <thead><tr><th scope="col">日付</th><th scope="col">案件</th><th scope="col">前の担当</th><th scope="col">次の担当</th><th scope="col">HubSpot への反映</th></tr></thead>
                    <tbody>
                      {(d.handover?.rows ?? []).slice(0, 100).map((r, i) => (
                        <tr key={`${r.deal_id}-${r.date ?? ''}-${String(i)}`}>
                          <td>{r.date ?? DASH}</td>
                          <td><button type="button" className="cv2-link" onClick={() => { go({ view: 'detail', id: r.deal_id }); }}>{orText(r.name, '取引名なし')}</button></td>
                          <td>{orText(r.from_label, '（不明）')}</td>
                          <td>{orText(r.to_label, '（不明）')}{r.to_retired === true ? <span className="cv2-tag cv2-tag-red">退職者</span> : null}</td>
                          <td>{orText(r.reflected, DASH)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
                {(d.pending_recordings?.rows ?? []).length > 0 && (
                  <p className="cv2-dim">録画のうち案件に結び付いていないもの {num(d.pending_recordings?.rows?.length ?? 0)} 件があります。</p>
                )}
              </Section>
            </>
          );
        }}
      </Async>
      <DataQuality />
      <Mtg result={mtg} retry={retry} />
      <Phone result={phone} retry={retry} go={go} />
    </>
  );
}
