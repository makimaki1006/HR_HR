// /app/admin?view=talk: トークの振り返り (ADR-033)。人ごとの「できていない割合」の表 → 電話の一覧 → 1 本の詳細。
// データは GET /api/crm/talk-review/* (管理者だけ)。&mode=fixture で架空の数字。
import { useEffect, useState, type ReactNode } from 'react';
import { ApiAbortedError, ApiHttpError, AuthRequiredError, apiGet, type ApiError } from '../../api/client';
import { redirectToLogin } from '../../shell/navigation';
import type { TalkCallDetailResponse } from '../../generated/TalkCallDetailResponse';
import type { TalkCallRow } from '../../generated/TalkCallRow';
import type { TalkCallsResponse } from '../../generated/TalkCallsResponse';
import type { TalkCheckDef } from '../../generated/TalkCheckDef';
import type { TalkPersonRow } from '../../generated/TalkPersonRow';
import type { TalkSummaryResponse } from '../../generated/TalkSummaryResponse';
import {
  CALL_STATUS_TEXT, MARK_LABEL, MARK_TEXT, PERIOD_LABELS, activePeriod, describeDays, fixtureCalls, fixtureDetail, fixtureSummary,
  fmtCallTime, fmtPct, fmtSeconds, jstToday, periodRange, ruleSentences, sortPeople,
  type TalkSort,
} from './talkModel';
import type { AdminRoute } from './route';
import { describeApiError } from './useApiGet';

export type TalkRoute = Extract<AdminRoute, { view: 'talk' }>;

function isFixture(): boolean {
  return typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('mode') === 'fixture';
}

/** 集計の数え始めで 503 counting が返ったときに取り直す間隔と回数 */
export const TALK_COUNTING_RETRY_MS = 4000;
export const TALK_COUNTING_MAX_TRIES = 15;
/** 一部の日を数えている途中の応答を、取り直す間隔と回数 (数え終わると消える) */
export const TALK_RECOUNT_MS = 15_000;
export const TALK_RECOUNT_MAX_TRIES = 80;

function countingDaysOf(data: unknown): string[] {
  const d = (data as { counting_days?: unknown } | null)?.counting_days;
  return Array.isArray(d) ? d.filter((x): x is string => typeof x === 'string') : [];
}

type TalkState<T> =
  | { status: 'loading' }
  | { status: 'counting' }
  | { status: 'not_configured' }
  | { status: 'ok'; data: T }
  | { status: 'error'; error: ApiError };

function errorKind(e: ApiError): string | undefined {
  return e instanceof ApiHttpError && e.status === 503 ? (e.body as { error_kind?: string } | undefined)?.error_kind : undefined;
}

/** GET して、503 counting は数秒おきに取り直す (上限あり)。503 not_configured は「まだ動いていません」用の状態にする */
function useTalkGet<T>(path: string): TalkState<T> {
  const [result, setResult] = useState<{ path: string; state: TalkState<T> } | null>(null);
  useEffect(() => {
    const ctl = new AbortController();
    let timer: ReturnType<typeof setTimeout> | null = null;
    const put = (state: TalkState<T>) => { setResult({ path, state }); };
    const load = (n: number): void => {
      void apiGet<T>(path, { signal: ctl.signal, timeoutMs: 30_000 }).then(r => {
        if (ctl.signal.aborted) return;
        if (r.ok) {
          put({ status: 'ok', data: r.data });
          if (countingDaysOf(r.data).length > 0 && n < TALK_RECOUNT_MAX_TRIES) {
            timer = setTimeout(() => { load(n + 1); }, TALK_RECOUNT_MS);
          }
          return;
        }
        if (r.error instanceof AuthRequiredError) { redirectToLogin(); return; }
        if (r.error instanceof ApiAbortedError) return;
        const kind = errorKind(r.error);
        if (kind === 'not_configured') { put({ status: 'not_configured' }); return; }
        if (kind === 'counting' && n < TALK_COUNTING_MAX_TRIES) {
          put({ status: 'counting' });
          timer = setTimeout(() => { load(n + 1); }, TALK_COUNTING_RETRY_MS);
          return;
        }
        put({ status: 'error', error: r.error });
      });
    };
    load(1);
    return () => {
      ctl.abort();
      if (timer !== null) clearTimeout(timer);
    };
  }, [path]);
  return result !== null && result.path === path ? result.state : { status: 'loading' };
}

// eslint-disable-next-line @typescript-eslint/no-unnecessary-type-parameters
function Live<T>({ path, children }: { path: string; children: (data: T) => ReactNode }) {
  const state = useTalkGet<T>(path);
  if (state.status === 'loading') return <p className="w8-subtle" role="status" data-testid="admin-loading">読み込み中…</p>;
  if (state.status === 'counting') return <p className="w8-subtle" role="status" data-testid="talk-counting">集計しています…（数字を数え終わるまで、少し待ってください）</p>;
  if (state.status === 'not_configured') return <p className="w8-alert" role="status" data-testid="talk-disabled">まだ動いていません。トークの振り返りの設定が済むと、ここに表示されます。</p>;
  if (state.status === 'error') {
    if (errorKind(state.error) === 'counting') {
      return <div role="alert" className="w8-alert" data-testid="admin-error"><p>集計がまだ終わっていません。しばらくしてから開き直してください。</p></div>;
    }
    const t = describeApiError(state.error);
    return (
      <div role="alert" className="w8-alert" data-testid="admin-error">
        <p>{t.title}: {t.detail}</p>
      </div>
    );
  }
  return <>{children(state.data)}</>;
}

/** 本番は API、fixture のときは架空の数字 */
function Loader<T>({ path, fixture, children }: { path: string; fixture: (() => T) | null; children: (data: T) => ReactNode }) {
  if (fixture !== null) return <>{children(fixture())}</>;
  return <Live<T> path={path}>{children}</Live>;
}

// ------------------------------------------------------------ 表

function Cell({ flagged, children, className }: { flagged: boolean; children: ReactNode; className?: string }) {
  const cls = [className, flagged ? 'talk-flag' : ''].filter(Boolean).join(' ');
  return <td className={cls === '' ? undefined : cls} data-flagged={flagged ? 'true' : undefined}>{children}</td>;
}

function PersonRowView({ p, checks, total, onOpen }: { p: TalkPersonRow; checks: TalkCheckDef[]; total: boolean; onOpen?: (id: string) => void }) {
  return (
    <tr className={total ? 'talk-total' : undefined} data-testid={total ? 'talk-total-row' : 'talk-row'}>
      <td>
        {total || p.owner_id === null || onOpen === undefined ? p.name : (
          <button type="button" className="talk-namebtn" onClick={() => { onOpen(p.owner_id ?? ''); }}>{p.name}</button>
        )}
      </td>
      <td className="w8-muted">{p.team ?? '-'}</td>
      <td className="w8-right">{p.dials}</td>
      <td className="w8-right">{p.appointments}</td>
      <Cell flagged={p.appointment_rate_flagged} className="w8-right">{fmtPct(p.appointment_rate_pct, 2)}</Cell>
      <td className="w8-right">
        {p.reviewed}
        <span className="talk-small">対象 {p.eligible} 本 ・ 待ち {p.waiting} ・ 取れない {p.unavailable}</span>
      </td>
      {checks.map(c => {
        const it = p.items.find(i => i.id === c.id);
        return (
          <Cell key={c.id} flagged={it?.flagged === true} className="w8-right">
            {fmtPct(it?.rate_pct ?? null)}
            <span className="talk-small">{it === undefined ? '' : `${String(it.not_done)}/${String(it.applicable)}本`}</span>
          </Cell>
        );
      })}
    </tr>
  );
}

/** 並びのまま、同じ場面が続く項目をまとめる */
function stageGroups(checks: TalkCheckDef[]): { stage: string; count: number; start: number }[] {
  const out: { stage: string; count: number; start: number }[] = [];
  checks.forEach((c, i) => {
    const last = out[out.length - 1];
    if (last?.stage === c.stage) last.count += 1;
    else out.push({ stage: c.stage, count: 1, start: i });
  });
  return out;
}

export function TalkSummaryTable({ data, onOpen }: { data: TalkSummaryResponse; onOpen: (ownerId: string) => void }) {
  const [sort, setSort] = useState<TalkSort | null>(null);
  const people = sortPeople(data.people, sort);
  const toggle = (key: TalkSort['key']) => {
    setSort(prev => {
      const same = prev !== null && JSON.stringify(prev.key) === JSON.stringify(key);
      return { key, dir: same && prev.dir === 'desc' ? 'asc' : 'desc' };
    });
  };
  const ariaSort = (key: TalkSort['key']): 'ascending' | 'descending' | 'none' =>
    sort !== null && JSON.stringify(sort.key) === JSON.stringify(key) ? (sort.dir === 'asc' ? 'ascending' : 'descending') : 'none';
  const arrow = (key: TalkSort['key']): string => (ariaSort(key) === 'none' ? '' : ariaSort(key) === 'ascending' ? ' ▲' : ' ▼');
  if (!data.enabled) {
    return <p className="w8-alert" role="status" data-testid="talk-disabled">まだ動いていません。トークの振り返りの設定が済むと、ここに人ごとの表が出ます。</p>;
  }
  return (
    <>
      <div className="w8-table-wrap">
        <table className="w8-table w8-table-dense talk-table" data-testid="talk-table">
          <thead>
            <tr>
              <th colSpan={6} className="talk-group-blank" />
              {stageGroups(data.checks).map(g => <th key={g.stage + String(g.start)} colSpan={g.count} className="talk-group">{g.stage}</th>)}
            </tr>
            <tr>
              <th>名前</th>
              <th>チーム</th>
              <th className="w8-right">総架電数</th>
              <th className="w8-right">アポ数</th>
              <th className="w8-right" aria-sort={ariaSort({ kind: 'rate' })}>
                <button type="button" className="w8-sort" onClick={() => { toggle({ kind: 'rate' }); }}>アポ率{arrow({ kind: 'rate' })}</button>
              </th>
              <th className="w8-right">判定した電話数</th>
              {data.checks.map(c => {
                const key = { kind: 'item', id: c.id } as const;
                return (
                  <th key={c.id} className="w8-right talk-itemhead" title={c.rule} aria-sort={ariaSort(key)}>
                    <button type="button" className="w8-sort" onClick={() => { toggle(key); }}>{c.title}{arrow(key)}</button>
                    <span className="talk-base">{c.baseline_pct === null ? '全体 -' : `全体 ${fmtPct(c.baseline_pct)}`}</span>
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {people.map(p => <PersonRowView key={p.owner_id ?? p.name} p={p} checks={data.checks} total={false} onOpen={onOpen} />)}
            {people.length === 0 && <tr><td colSpan={6 + data.checks.length} className="w8-muted">この期間に判定できた電話はまだありません。</td></tr>}
            <PersonRowView p={data.total} checks={data.checks} total />
          </tbody>
        </table>
      </div>
      <p className="talk-rules" data-testid="talk-rules">
        {ruleSentences(data.rules).map(s => <span key={s} style={{ display: 'block' }}>{s}</span>)}
        <span style={{ display: 'block' }}>各項目の割合は「できていない電話 ÷ その項目に当てはまる電話」で、小さい数字はその本数です。見出しの「全体」は資料にある全体の割合です。</span>
        <span style={{ display: 'block' }} data-testid="talk-sampling">割合はすべて、判定した電話の中での割合です（全部の電話の割合ではありません）。判定は 1 人 1 日あたり新しい電話から {data.per_person_day} 本までで、「対象」は判定の対象になる 70 秒以上の電話の本数です。</span>
      </p>
    </>
  );
}

// ------------------------------------------------------------ 電話の一覧

function Mark({ status }: { status: string }) {
  return <span className={`talk-mark-${status}`} aria-label={MARK_LABEL[status] ?? status} title={MARK_LABEL[status] ?? status}>{MARK_TEXT[status] ?? status}</span>;
}

export function TalkCallsTable({ data, onOpen }: { data: TalkCallsResponse; onOpen: (callId: string) => void }) {
  return (
    <div className="w8-table-wrap">
      <table className="w8-table w8-table-dense talk-calls" data-testid="talk-calls-table">
        <thead>
          <tr>
            <th colSpan={3} className="talk-group-blank" />
            {stageGroups(data.checks).map(g => <th key={g.stage + String(g.start)} colSpan={g.count} className="talk-group">{g.stage}</th>)}
            <th className="talk-group-blank" />
          </tr>
          <tr>
            <th>日時</th>
            <th className="w8-right">通話時間</th>
            <th>結果</th>
            {data.checks.map(c => <th key={c.id} title={c.rule} className="talk-itemhead">{c.title}</th>)}
            <th>直すとよい点</th>
          </tr>
        </thead>
        <tbody>
          {data.calls.map((c: TalkCallRow) => (
            <tr key={c.hubspot_call_id} data-testid="talk-call-row">
              <td style={{ whiteSpace: 'nowrap' }}><button type="button" className="talk-namebtn" onClick={() => { onOpen(c.hubspot_call_id); }}>{fmtCallTime(c.call_at)}</button></td>
              <td className="w8-right" style={{ whiteSpace: 'nowrap' }}>{fmtSeconds(c.duration_sec)}</td>
              <td>{c.status === 'judged' ? c.outcome ?? '-' : <span className="w8-muted">{CALL_STATUS_TEXT[c.status] ?? c.status}</span>}</td>
              {data.checks.map(ch => {
                const m = c.items.find(i => i.id === ch.id);
                return <td key={ch.id}>{c.status === 'judged' && m !== undefined ? <Mark status={m.status} /> : ''}</td>;
              })}
              <td>{c.summary ?? ''}</td>
            </tr>
          ))}
          {data.calls.length === 0 && <tr><td colSpan={4 + data.checks.length} className="w8-muted">この期間の電話はありません。</td></tr>}
        </tbody>
      </table>
    </div>
  );
}

// ------------------------------------------------------------ 詳細

export function TalkDetailView({ data }: { data: TalkCallDetailResponse }) {
  const checkOf = (id: string) => data.checks.find(c => c.id === id);
  // できていない項目の根拠の発言 → 項目名
  const evidence = new Map<number, string[]>();
  for (const it of data.items) {
    if (it.status !== 'not_done') continue;
    for (const n of it.evidence) evidence.set(n, [...(evidence.get(n) ?? []), checkOf(it.id)?.title ?? it.id]);
  }
  const c = data.call;
  return (
    <>
      <p className="w8-subtle">
        {data.rep_name} ・ {fmtCallTime(c.call_at)} ・ {fmtSeconds(c.duration_sec)}
        {c.status === 'judged' ? ` ・ ${c.outcome ?? '-'}` : ` ・ ${CALL_STATUS_TEXT[c.status] ?? c.status}`}
        {' ・ '}<a className="w8-link" href={data.hubspot_url} target="_blank" rel="noreferrer">HubSpot で開く</a>
      </p>
      <div className="talk-detail-grid">
        <section data-testid="talk-items">
          <h2 className="w8-section-title">項目ごとの判定</h2>
          {data.items.length === 0 && <p className="w8-muted">この電話はまだ判定されていません。</p>}
          {data.items.map(it => (
            <div key={it.id} className={`talk-item ${it.status}`} data-testid="talk-item">
              <div><Mark status={it.status} /> <strong>{checkOf(it.id)?.title ?? it.id}</strong> <span className="w8-muted w8-xs">{MARK_LABEL[it.status] ?? it.status}</span></div>
              {it.quote !== null && <div className="talk-quote">「{it.quote}」{it.evidence.length > 0 && <span className="w8-xs w8-muted"> （発言 {it.evidence.join('・')}）</span>}</div>}
              {it.advice !== null && <div className="talk-advice">次に言うとよいこと: {it.advice}</div>}
              {it.note !== null && <div className="w8-xs w8-muted">{it.note}</div>}
            </div>
          ))}
        </section>
        <section data-testid="talk-transcript">
          <h2 className="w8-section-title">文字起こし</h2>
          {data.transcript === null ? (
            <p className="w8-alert" role="status">{data.transcript_error ?? '文字起こしを取れませんでした。'}</p>
          ) : (
            data.transcript.map(t => {
              const tags = evidence.get(t.n);
              const cls = ['talk-turn', t.speaker, tags !== undefined ? 'talk-evidence' : ''].filter(Boolean).join(' ');
              return (
                <div key={t.n} className={cls} data-testid="talk-turn" data-evidence={tags !== undefined ? 'true' : undefined}>
                  <span className="talk-who">{t.speaker === 'rep' ? '営業' : '先方'}</span>
                  <span>
                    {t.text}
                    {tags !== undefined && <span className="talk-evidence-tag">できていない根拠: {tags.join('・')}</span>}
                  </span>
                </div>
              );
            })
          )}
        </section>
      </div>
    </>
  );
}

// ------------------------------------------------------------ 画面

function Controls({ route, navigate, teams, from, to, today }: {
  route: TalkRoute; navigate: (r: AdminRoute) => void; teams: string[]; from: string; to: string; today: string;
}) {
  const active = activePeriod(from, to, today);
  const go = (patch: Partial<TalkRoute>) => { navigate({ ...route, owner: '', call: '', ...patch }); };
  return (
    <div className="talk-bar">
      <div role="group" aria-label="期間" className="talk-period">
        {PERIOD_LABELS.map(([k, label]) => (
          <button key={k} type="button" aria-pressed={active === k} onClick={() => { go(periodRange(k, today)); }}>{label}</button>
        ))}
      </div>
      <label className="w8-subtle" style={{ margin: 0 }}>
        日付指定{' '}
        <input type="date" aria-label="開始日" value={from} max={to} onChange={e => { if (e.target.value !== '') go({ from: e.target.value, to: to < e.target.value ? e.target.value : to }); }} />
        {' 〜 '}
        <input type="date" aria-label="終了日" value={to} min={from} onChange={e => { if (e.target.value !== '') go({ to: e.target.value }); }} />
      </label>
      <label className="w8-subtle" style={{ margin: 0 }}>
        チーム{' '}
        <select value={route.team} aria-label="チームで絞り込む" onChange={e => { go({ team: e.target.value }); }}>
          <option value="">すべて</option>
          {teams.map(t => <option key={t} value={t}>{t}</option>)}
        </select>
      </label>
    </div>
  );
}

export function TalkPanel({ route, navigate, fixture }: { route: TalkRoute; navigate: (r: AdminRoute) => void; fixture?: boolean }) {
  const fx = fixture ?? isFixture();
  const today = jstToday();
  const from = route.from === '' ? today : route.from;
  const to = route.to === '' ? from : route.to;
  const q = `from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`;
  const withPeriod = { ...route, from, to };

  const title = <h1 className="w8-h1" style={{ marginBottom: 4 }}>トークの振り返り</h1>;
  const banner = fx ? <p className="w8-alert" role="status">架空サンプルを表示中です。</p> : null;

  if (route.owner !== '' && route.call !== '') {
    return (
      <>
        {title}{banner}
        <button type="button" className="talk-back" onClick={() => { navigate({ ...withPeriod, call: '' }); }}>← 電話の一覧へ戻る</button>
        <Loader<TalkCallDetailResponse> path={`/api/crm/talk-review/calls/${encodeURIComponent(route.call)}`} fixture={fx ? () => fixtureDetail(route.call) : null}>
          {d => <TalkDetailView data={d} />}
        </Loader>
      </>
    );
  }
  if (route.owner !== '') {
    return (
      <>
        {title}{banner}
        <button type="button" className="talk-back" onClick={() => { navigate({ ...withPeriod, owner: '', call: '' }); }}>← 全員の表へ戻る</button>
        <Loader<TalkCallsResponse> path={`/api/crm/talk-review/calls?owner=${encodeURIComponent(route.owner)}&${q}`} fixture={fx ? () => fixtureCalls(route.owner, from, to) : null}>
          {d => (
            <>
              <p className="w8-subtle" data-testid="talk-calls-head">{d.name} の電話（{d.from} 〜 {d.to}、{d.calls.length} 本）。日時を押すと中身が見られます。</p>
              <TalkCallsTable data={d} onOpen={id => { navigate({ ...withPeriod, call: id }); }} />
            </>
          )}
        </Loader>
      </>
    );
  }
  const teamQ = route.team === '' ? '' : `&team=${encodeURIComponent(route.team)}`;
  return (
    <>
      {title}{banner}
      <Loader<TalkSummaryResponse> path={`/api/crm/talk-review/summary?${q}${teamQ}`} fixture={fx ? () => fixtureSummary(from, to, route.team) : null}>
        {d => (
          <>
            <Controls route={route} navigate={navigate} teams={d.teams} from={from} to={to} today={today} />
            <p className="w8-subtle">
              {d.from === d.to ? d.from : `${d.from} 〜 ${d.to}`}（日本時間）。名前を押すと、その人の電話の一覧が見られます。
            </p>
            {d.counting_days.length > 0 && (
              <p className="w8-alert" role="status" data-testid="talk-counting-days">
                {describeDays(d.counting_days)} の総架電とアポは数えている途中です。数え終わるまで、その日の分は表に入っていません（数え終わると自動で更新します）。
              </p>
            )}
            <TalkSummaryTable data={d} onOpen={id => { navigate({ ...withPeriod, owner: id, call: '' }); }} />
          </>
        )}
      </Loader>
    </>
  );
}
