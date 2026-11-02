import { useState, type ReactNode } from 'react';
import { apiGet } from '../../api/client';
import { DASH, dayDiff, durText, monthsBefore, num, orText, yen } from './format';
import { Async, CaseLink, FlagChips, NoData, Section } from './parts';
import type { Route } from './route';
import type { DetailEvent, DetailResponse, GoalsResponse } from './types';
import { useApi } from './useApi';
import { useEffect } from 'react';

const RECENT_MONTHS = 3;
const GOAL_PROPS = {
  ideal: 'shokai_mtg_mokuhyou_risou',
  minimum: 'shokai_mtg_mokuhyou_zenshin',
  data: 'shokai_mtg_mokuhyou_data',
} as const;

type KindKey = 'mtg' | 'email' | 'call' | 'handover';
const KINDS: { k: KindKey; t: string }[] = [
  { k: 'mtg', t: 'MTG' },
  { k: 'email', t: 'メール' },
  { k: 'call', t: '電話' },
  { k: 'handover', t: '交代' },
];

function kindOf(e: DetailEvent): KindKey | 'other' {
  if (e.kind === 'mail_mtg' || e.kind === 'zoom_mtg' || e.kind === 'mtg') return 'mtg';
  if (e.kind === 'email' || e.kind === 'call' || e.kind === 'handover') return e.kind;
  return 'other';
}

/** この契約の期間の出来事か (付け直して来たものは入れる。契約期間の外のものは入れない) */
function isOwn(e: DetailEvent): boolean {
  const s = e.attach?.state;
  return !e.attach || s === undefined || s === 'own' || s === 'moved_in';
}

function hasText(...v: (string | null | undefined)[]): boolean {
  return v.some((x) => x !== null && x !== undefined && x !== '');
}

function LongText({ text }: { text: string | null | undefined }) {
  const t = text ?? '';
  if (t === '') return <span className="cv2-dim">記載なし</span>;
  if (t.length <= 120) return <>{t}</>;
  return (
    <details className="cv2-long">
      <summary>
        {t.slice(0, 120)}… <span className="cv2-dim">全文を表示</span>
      </summary>
      <div>{t}</div>
    </details>
  );
}

function Pairs({ rows }: { rows: [string, string | null | undefined][] }) {
  const shown = rows.filter(([, v]) => v !== null && v !== undefined && v !== '');
  if (shown.length === 0) return null;
  return (
    <dl className="cv2-pairs">
      {shown.map(([k, v]) => (
        <div key={k}>
          <dt>{k}</dt>
          <dd><LongText text={v} /></dd>
        </div>
      ))}
    </dl>
  );
}

function attachNote(e: DetailEvent): ReactNode {
  const a = e.attach;
  if (a?.state === undefined || a.state === 'own') return null;
  if (a.state === 'moved_in') {
    return <p className="cv2-via">付け直し: 同じ拠点の別の取引「{a.moved_from?.name ?? '取引名なし'}」から</p>;
  }
  if (a.state === 'moved_out') {
    return <p className="cv2-via">この取引に付いていますが、契約期間の外の日です。「{a.moved_to?.name ?? '取引名なし'}」の期間の出来事です（接触には数えていません）。</p>;
  }
  return <p className="cv2-via">契約期間の外の日です。接触には数えていません。</p>;
}

function EventItem({ e }: { e: DetailEvent }) {
  const k = e.kind;
  let head: ReactNode = null;
  let body: ReactNode = null;
  if (k === 'mtg') {
    head = (
      <>
        <span className="cv2-mark">MTG</span>
        <small>{e.source_label}</small>
        {e.minutes !== null && e.minutes !== undefined ? <small>{num(e.minutes)}分</small> : null}
        {e.host ? <small>ホスト {e.host}</small> : null}
        {e.mtg_type ? <span className="cv2-tag">{e.mtg_type}</span> : null}
      </>
    );
    body = (
      <>
        {e.link_certainty && e.link_certainty !== '高' ? (
          <p className="cv2-warnbox">
            取引への結び付け: 確度 {e.link_certainty}。録画があったことは事実ですが、この取引の MTG だという結び付けは推定です
            {e.link_reason ? `（${e.link_reason}）` : ''}。
          </p>
        ) : null}
        {e.extracted === true ? (
          <>
            <Pairs rows={[['やること', e.todo], ['顧客の懸念', e.concern]]} />
            <details className="cv2-more">
              <summary>記録の詳細</summary>
              <Pairs rows={[['前向きシグナル', e.positive], ['リスク判定', e.risk ? `${e.risk}${e.risk_reason ? `（${e.risk_reason}）` : ''}` : null], ['次回予定', e.next]]} />
            </details>
          </>
        ) : (
          <p className="cv2-via"><span className="cv2-tag">未抽出</span> やること・懸念などはまだ抽出していません</p>
        )}
      </>
    );
  } else if (k === 'zoom_mtg') {
    head = (
      <>
        <span className="cv2-mark">MTG</span>
        <small>（Zoom の開催記録・事実）{e.time ? ` ${e.time}${e.end_time ? `〜${e.end_time}` : ''}` : ''}</small>
      </>
    );
  } else if (k === 'mail_mtg') {
    head = (
      <>
        <span className="cv2-mark cv2-mark-est" title="メールの文面から起こした実施日です。録画のような中身はありません">MTG（推定）</span>
        <small>{e.source_label}</small>
      </>
    );
    body = e.same_day_recording === true ? <p className="cv2-via"><b>同じ日に録画の MTG があります</b>（同じ MTG の見込みが高い）</p> : null;
  } else if (k === 'email') {
    head = (
      <>
        <span className="cv2-mark">メール</span>
        <small>{e.direction ?? ''}</small>
        {e.mail_type ? <span className="cv2-tag">{e.mail_type}</span> : null}
      </>
    );
    body = <p className="cv2-subj">{orText(e.subject, '件名なし')}</p>;
  } else if (k === 'call') {
    const who = e.handler_label ?? e.handler ?? e.owner;
    head = (
      <>
        <span className="cv2-mark">電話</span>
        <small>{e.source_label}</small>
        <b>{durText(e.duration_sec)}</b>
        {e.contact === true ? null : <span className="cv2-tag">60秒以下</span>}
        {e.direction === 'outbound' ? <small>発信</small> : e.direction === 'inbound' ? <small>着信</small> : null}
        {who ? <small>{e.handler ? '話した人' : '担当'} {who}</small> : null}
        <small>文字起こし{e.has_transcript === true ? 'あり' : 'なし'}</small>
      </>
    );
    const s = e.summary;
    body = s?.summary ? (
      <dl className="cv2-pairs">
        <div><dt>AI 要約（誤りを含むことがあります）</dt><dd><LongText text={s.summary} /></dd></div>
        <div><dt>次にやること</dt><dd><LongText text={s.next_action} /></dd></div>
        <div><dt>顧客の懸念</dt><dd><LongText text={s.concern} /></dd></div>
      </dl>
    ) : (
      <p className="cv2-via"><span className="cv2-tag">要約なし</span></p>
    );
  } else if (k === 'handover') {
    head = (
      <>
        <span className="cv2-mark">交代</span>
        <small>{e.source_label}</small>
      </>
    );
    body = (
      <p className="cv2-via">
        {orText(e.from, '（不明）')} → {orText(e.to, '（不明）')}
        {e.to_retired === true ? <span className="cv2-tag cv2-tag-red">退職者</span> : null}
        {e.reflected ? ` HubSpot への反映: ${e.reflected}` : ''}
        {e.record_gap_days !== null && e.record_gap_days !== undefined ? `（担当欄の書き換えとのずれ ${num(e.record_gap_days)}日）` : ''}
      </p>
    );
  }
  return (
    <li className={`cv2-event cv2-event-${k === 'mail_mtg' || k === 'zoom_mtg' ? 'mtg' : k}`}>
      <div className="cv2-event-head">
        <div className="cv2-event-kind">{head}</div>
        <div className="cv2-event-when">
          <b>{e.date}</b> {e.time ?? ''}
        </div>
      </div>
      <div className="cv2-event-body">
        {body}
        {attachNote(e)}
      </div>
    </li>
  );
}

function Goals({ dealId }: { dealId: string }) {
  const [state, setState] = useState<{ id: string; v: GoalsResponse | 'error' } | null>(null);
  useEffect(() => {
    const ctl = new AbortController();
    const props = Object.values(GOAL_PROPS).join(',');
    void apiGet<GoalsResponse>(`/api/crm/workspace/deals/${encodeURIComponent(dealId)}?deal_props=${props}`, { signal: ctl.signal }).then((r) => {
      if (!ctl.signal.aborted) setState({ id: dealId, v: r.ok ? r.data : 'error' });
    });
    return () => { ctl.abort(); };
  }, [dealId]);
  const lines = (t: string | null | undefined): string[] => (t ?? '').split('\n').map((x) => x.trim()).filter((x) => x !== '');
  let inner: ReactNode;
  if (state?.id !== dealId) {
    inner = <p className="cv2-dim">読み込んでいます…</p>;
  } else if (state.v === 'error' || (state.v.partial ?? []).some((p) => p.part === 'selected_properties')) {
    inner = <p className="cv2-dim">HubSpot から読めませんでした。</p>;
  } else {
    const deal = state.v.selected?.deal ?? {};
    const ideal = lines(deal[GOAL_PROPS.ideal]);
    const minimum = lines(deal[GOAL_PROPS.minimum]);
    inner =
      ideal.length === 0 && minimum.length === 0 ? (
        <NoData label="初回MTGの目標（理想・前進ライン）" />
      ) : (
        <div className="cv2-goals">
          <div><h3>理想（頑張ればできそうな目標）</h3><ul>{ideal.map((x) => <li key={x}>{x}</li>)}</ul>{ideal.length === 0 ? <span className="cv2-dim">未入力</span> : null}</div>
          <div><h3>前進ライン（必ず達成する目標）</h3><ul>{minimum.map((x) => <li key={x}>{x}</li>)}</ul>{minimum.length === 0 ? <span className="cv2-dim">未入力</span> : null}</div>
        </div>
      );
  }
  return (
    <Section title="初回MTGの目標" aside={<CaseLink dealId={dealId} label="案件管理で入力・訂正する" />}>
      {inner}
    </Section>
  );
}

function Search({ go }: { go: (r: Partial<Route>) => void }) {
  const [q, setQ] = useState('');
  const [submitted, setSubmitted] = useState('');
  const path = submitted === '' ? null : `/api/consulting/deal-detail?q=${encodeURIComponent(submitted)}`;
  const result = useApi<{ search?: { n_match?: number; rows?: { deal_id: string; name?: string; site?: string; stage?: string; consultant?: string | null; is_active?: boolean }[] } }>(path);
  return (
    <Section title="案件を探す">
      <div className="cv2-filters">
        <label className="cv2-field">
          案件名・拠点名の一部
          <input value={q} onChange={(e) => { setQ(e.target.value); }} onKeyDown={(e) => { if (e.key === 'Enter' && !e.nativeEvent.isComposing) setSubmitted(q.trim()); }} />
        </label>
        <button type="button" onClick={() => { setSubmitted(q.trim()); }}>探す</button>
      </div>
      <p className="cv2-dim">案件一覧や今日の画面で案件名を押しても開けます。</p>
      {path !== null && (
        <Async result={result} onRetry={() => { setSubmitted(`${submitted} `.trim()); }}>
          {(d) => {
            const rows = d.search?.rows ?? [];
            return rows.length === 0 ? (
              <p className="cv2-empty">見つかりませんでした。</p>
            ) : (
              <ul className="cv2-results">
                {rows.map((r) => (
                  <li key={r.deal_id}>
                    <button type="button" className="cv2-link" onClick={() => { go({ view: 'detail', id: r.deal_id }); }}>{orText(r.name, '取引名なし')}</button>
                    <small> {r.site ?? ''} {r.stage ?? ''} {r.consultant ?? ''}{r.is_active === true ? '' : ' 稼働中ではない'}</small>
                  </li>
                ))}
              </ul>
            );
          }}
        </Async>
      )}
    </Section>
  );
}

function Body({ d, go }: { d: DetailResponse; go: (r: Partial<Route>) => void }) {
  const [kinds, setKinds] = useState<Record<KindKey, boolean>>({ mtg: true, email: true, call: true, handover: true });
  const [allCalls, setAllCalls] = useState(false);
  const [applications, setApplications] = useState(false);
  const [older, setOlder] = useState(false);
  const deal = d.deal;
  if (!deal) return null;
  const today = d.meta.today ?? '';
  const events = d.events ?? [];
  const ownEvents = events.filter(isOwn);
  const ct = ownEvents.find((e) => e.kind === 'mtg' || e.kind === 'zoom_mtg' || (e.kind === 'call' && e.contact === true));
  const mt = ownEvents.find((e) => e.kind === 'mtg' || e.kind === 'zoom_mtg' || e.kind === 'mail_mtg');
  const sm = ownEvents.find(
    (e) => (e.kind === 'call' && hasText(e.summary?.next_action, e.summary?.concern)) || (e.kind === 'mtg' && e.extracted === true && hasText(e.todo, e.concern)),
  );
  const ago = (dt: string): string => {
    const n = dayDiff(dt, today);
    return n === null ? '' : n <= 0 ? '（今日）' : `（${num(n)}日前）`;
  };
  const left = dayDiff(today, deal.expiration);
  const byKind = events.filter((e) => {
    const k = kindOf(e);
    if (k === 'other' || !kinds[k]) return false;
    if (e.kind === 'email' && !applications && e.mail_type === '応募の報告') return false;
    if (e.kind === 'call' && !allCalls && e.contact !== true) return false;
    return true;
  });
  const cut = monthsBefore(today, RECENT_MONTHS);
  const shown = older || cut === '' ? byKind : byKind.filter((e) => e.date >= cut);
  const nOlder = byKind.length - shown.length;
  const chain = d.chain?.rows ?? [];
  const at = chain.findIndex((r) => r.current === true);
  const nth = deal.renewal_no === null || deal.renewal_no === undefined ? '' : deal.renewal_no === 0 ? '（初回）' : `（${num(deal.renewal_no)}回目の継続）`;
  return (
    <>
      <div className="cv2-detail-head">
        <nav className="cv2-crumbs" aria-label="この案件の位置">
          <span>{deal.houjin_name ?? '法人の情報なし'}</span>
          <span aria-hidden="true">＞</span>
          <span>{orText(deal.site, '拠点名なし')}</span>
          <span aria-hidden="true">＞</span>
          <span aria-current="page">この案件{nth}</span>
        </nav>
        <h2>{orText(deal.name, '取引名なし')}</h2>
        <p className="cv2-attr">
          <span>ステージ {deal.stage ?? DASH}</span>
          <span>契約 {deal.start ?? DASH} 〜 {deal.expiration ?? DASH}{deal.period ? `（${num(deal.period)}か月）` : ''}</span>
          <span>金額 {yen(deal.amount)}</span>
          <span>{deal.is_active === true ? '稼働中' : '稼働中ではない'}{deal.right_censored === true ? '（結果は未確定）' : ''}</span>
        </p>
        <p className="cv2-attr">
          注意 {deal.flags === null || deal.flags === undefined ? <span className="cv2-dim">稼働中の案件にだけ付けています</span> : <FlagChips flags={deal.flags} max={6} />}
        </p>
        <p><CaseLink dealId={deal.deal_id} /></p>
      </div>

      <div className="cv2-facts">
        <div>
          <span>最後の接触（MTG・60秒超の電話）</span>
          {ct ? <><b>{ct.date}</b>{ago(ct.date)} {ct.kind === 'mtg' ? 'MTG（録画）' : ct.kind === 'zoom_mtg' ? 'MTG（Zoom 開催記録）' : `電話 ${durText(ct.duration_sec)}`}</> : <small>この契約の期間に接触の記録が見つかりません（接触していないとは限りません）</small>}
        </div>
        <div>
          <span>最後のMTG</span>
          {mt ? <><b>{mt.date}</b>{ago(mt.date)} {mt.kind === 'mtg' ? '録画（事実）' : mt.kind === 'zoom_mtg' ? 'Zoom 開催記録（事実）' : 'メール由来（推定）'}</> : <small>この契約の期間にMTGの記録が見つかりません（MTGをしていないとは限りません）</small>}
        </div>
        <div>
          <span>満了まで</span>
          {left === null ? DASH : left >= 0 ? <><b>{num(left)}日</b> <small>（{deal.expiration}）</small></> : <>満了日を過ぎています <small>（{deal.expiration}）</small></>}
        </div>
        <div>
          <span>担当</span>
          {orText(deal.consultant, DASH)}
          {deal.consultant_retired === true ? <span className="cv2-tag cv2-tag-red">退職者</span> : null}
        </div>
      </div>

      <Section title="次の対応と顧客の懸念">
        {sm ? (
          <>
            <Pairs
              rows={[
                ['次の対応', sm.kind === 'call' ? sm.summary?.next_action : sm.todo],
                ['顧客の懸念', sm.kind === 'call' ? sm.summary?.concern : sm.concern],
              ]}
            />
            <p className="cv2-dim">
              出典: {sm.kind === 'call' ? `直近の電話（${sm.date}）の AI 要約。誤りを含むことがあります。` : `直近の録画 MTG（${sm.date}）から抽出した項目。`}
            </p>
          </>
        ) : (
          <NoData label="次の対応・顧客の懸念" />
        )}
      </Section>

      <Goals dealId={deal.deal_id} />

      <Section title="接触履歴（新しい順）">
        {d.meta.summary_sheet !== 'ok' && (
          <p className="cv2-note">電話の要約はまだありません。電話は日時・長さ・文字起こしの有無だけ出しています。</p>
        )}
        <div className="cv2-filters" role="group" aria-label="履歴の絞り込み">
          <span>出す種類:</span>
          {KINDS.map((k) => (
            <label key={k.k} className="cv2-check">
              <input type="checkbox" checked={kinds[k.k]} onChange={(e) => { setKinds({ ...kinds, [k.k]: e.target.checked }); }} />
              {k.t}
            </label>
          ))}
          <label className="cv2-check">
            <input type="checkbox" checked={applications} onChange={(e) => { setApplications(e.target.checked); }} />
            応募の報告メールも出す
          </label>
          <span>電話:</span>
          <button type="button" aria-pressed={!allCalls} onClick={() => { setAllCalls(false); }}>60秒超だけ</button>
          <button type="button" aria-pressed={allCalls} onClick={() => { setAllCalls(true); }}>全部</button>
        </div>
        <p className="cv2-count" aria-live="polite">
          <b>{num(events.length)} 件中 {num(shown.length)} 件</b>を表示
          {nOlder > 0 ? `（直近${String(RECENT_MONTHS)}か月（${cut} 以降）だけ）` : byKind.length < events.length ? '（種類・電話の絞り込み）' : '（全部出しています）'}
        </p>
        <p className="cv2-dim">事実のほか、メール由来の MTG は推定です。付け直した記録は元の取引を併記します。</p>
        {shown.length > 0 ? (
          <ol className="cv2-timeline">
            {shown.map((e, i) => <EventItem key={`${e.kind}-${e.date}-${e.time ?? ''}-${String(i)}`} e={e} />)}
          </ol>
        ) : (
          <p className="cv2-empty">この条件で出せる出来事はありません。</p>
        )}
        {nOlder > 0 && <button type="button" onClick={() => { setOlder(true); }}>もっと前を出す（{cut} より前の {num(nOlder)} 件）</button>}
        {older && nOlder === 0 && cut !== '' && byKind.some((e) => e.date < cut) && (
          <button type="button" onClick={() => { setOlder(false); }}>直近{RECENT_MONTHS}か月だけに戻す</button>
        )}
      </Section>

      {chain.length > 1 && (
        <Section title="契約の連なり（同じ拠点の本体契約）">
          <p className="cv2-dim">{at >= 0 ? `いま ${num(at + 1)} 件目 / 全 ${num(chain.length)} 件（開始の古い順）` : '位置不明'}</p>
          <div className="cv2-table-wrap">
            <table className="cv2-table">
              <thead>
                <tr><th scope="col">案件</th><th scope="col">ステージ</th><th scope="col">開始〜満了</th><th scope="col" className="cv2-num">継続</th><th scope="col" className="cv2-num">金額</th><th scope="col">状態</th></tr>
              </thead>
              <tbody>
                {chain.map((r) => (
                  <tr key={r.deal_id}>
                    <td>
                      {r.current === true ? <><b>{r.name}</b><span className="cv2-tag">いま見ている</span></> : <button type="button" className="cv2-link" onClick={() => { go({ view: 'detail', id: r.deal_id }); }}>{orText(r.name, '取引名なし')}</button>}
                    </td>
                    <td>{r.stage}</td>
                    <td>{r.start ?? DASH} 〜 {r.expiration ?? DASH}</td>
                    <td className="cv2-num">{r.renewal_no === null || r.renewal_no === undefined ? DASH : num(r.renewal_no)}</td>
                    <td className="cv2-num">{yen(r.amount)}</td>
                    <td>{r.is_active === true ? '稼働中' : ''}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Section>
      )}
    </>
  );
}

export function DetailView({ route, go }: { route: Route; go: (r: Partial<Route>) => void }) {
  const [reload, setReload] = useState(0);
  const path = route.id === '' ? null : `/api/consulting/deal-detail?deal_id=${encodeURIComponent(route.id)}`;
  const result = useApi<DetailResponse>(path, reload);
  if (path === null) return <Search go={go} />;
  return (
    <>
      <p>
        <button type="button" onClick={() => { go({ view: 'board', id: '' }); }}>← 案件一覧へ戻る</button>
      </p>
      <Async result={result} onRetry={() => { setReload((n) => n + 1); }}>
        {(d) =>
          d.meta.found === true ? (
            <Body d={d} go={go} />
          ) : (
            <>
              <p className="cv2-note" role="status">{d.meta.reason ?? 'この案件は出せません。'}</p>
              <Search go={go} />
            </>
          )
        }
      </Async>
    </>
  );
}
