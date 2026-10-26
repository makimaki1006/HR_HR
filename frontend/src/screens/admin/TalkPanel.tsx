// /app/admin?view=talk: トークの振り返り (ADR-033)。人ごとの「できていない割合」の表 → 電話の一覧 → 1 本の詳細。
// データは GET /api/crm/talk-review/* (管理者だけ)。&mode=fixture で架空の数字。
import { useEffect, useState, type ReactNode } from 'react';
import { ApiAbortedError, ApiHttpError, AuthRequiredError, apiGet, type ApiError } from '../../api/client';
import { redirectToLogin } from '../../shell/navigation';
import type { DialBreakdown } from '../../generated/DialBreakdown';
import type { TalkCallDetailResponse } from '../../generated/TalkCallDetailResponse';
import type { TalkCallRow } from '../../generated/TalkCallRow';
import type { TalkCallsResponse } from '../../generated/TalkCallsResponse';
import type { TalkCheckDef } from '../../generated/TalkCheckDef';
import type { TalkPersonRow } from '../../generated/TalkPersonRow';
import type { TalkSummaryResponse } from '../../generated/TalkSummaryResponse';
import {
  CALL_STATUS_TEXT, MARK_LABEL, dialBreakdownRows, MARK_TEXT, PERIOD_LABELS, activePeriod, describeDays, fixtureCalls, fixtureDetail, fixtureSummary,
  fmtCallTime, fmtPct, fmtPerContact, fmtSeconds, jstToday, periodRange, sortPeople, donePct, judgeItem, judgeAppointment, ratioText, type CellJudgement,
  type TalkSort,
} from './talkModel';
import {
  diffClass, fmtMetric, joinPeople, metricDiff, parsePeople, previousPeriod, sumRows, togglePerson, validCompare, type Diff, type MetricId,
} from './talkCompare';
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

/** 発信の内訳を過去の日から取り込み中か (終わると消える) */
function breakdownPendingOf(data: unknown): boolean {
  return (data as { dial_breakdown_pending?: unknown } | null)?.dial_breakdown_pending === true;
}

type TalkState<T> =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'counting' }
  | { status: 'not_configured' }
  | { status: 'ok'; data: T }
  | { status: 'error'; error: ApiError };

function errorKind(e: ApiError): string | undefined {
  return e instanceof ApiHttpError && e.status === 503 ? (e.body as { error_kind?: string } | undefined)?.error_kind : undefined;
}

/** GET して、503 counting は数秒おきに取り直す (上限あり)。503 not_configured は「まだ動いていません」用の状態にする */
function useTalkGet<T>(path: string | null): TalkState<T> {
  const [result, setResult] = useState<{ path: string; state: TalkState<T> } | null>(null);
  useEffect(() => {
    if (path === null) return;
    const ctl = new AbortController();
    let timer: ReturnType<typeof setTimeout> | null = null;
    const put = (state: TalkState<T>) => { setResult({ path, state }); };
    const load = (n: number): void => {
      void apiGet<T>(path, { signal: ctl.signal, timeoutMs: 30_000 }).then(r => {
        if (ctl.signal.aborted) return;
        if (r.ok) {
          put({ status: 'ok', data: r.data });
          if ((countingDaysOf(r.data).length > 0 || breakdownPendingOf(r.data)) && n < TALK_RECOUNT_MAX_TRIES) {
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
  if (path === null) return { status: 'idle' };
  return result !== null && result.path === path ? result.state : { status: 'loading' };
}

// eslint-disable-next-line @typescript-eslint/no-unnecessary-type-parameters
function Live<T>({ path, children }: { path: string; children: (data: T) => ReactNode }) {
  const state = useTalkGet<T>(path);
  if (state.status === 'idle') return null;
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

function Cell({ j, children, className }: { j: CellJudgement | null; children: ReactNode; className?: string }) {
  const stateCls = j?.state === 'red' ? 'talk-flag' : j?.state === 'grey' ? 'talk-few' : '';
  const cls = [className, stateCls].filter(Boolean).join(' ');
  return <td className={cls === '' ? undefined : cls} data-flagged={j?.state === 'red' ? 'true' : undefined} data-state={j?.state} title={j?.reason}>{children}</td>;
}

/** 発信数のセル。マウスを当てる・キーボードで止まると、発信の内訳 (Zoom の記録から数えたもの) を表で出す */
function OutboundCell({ value, breakdown, pending, extra }: { value: number | null; breakdown: DialBreakdown | null; pending: boolean; extra?: ReactNode }) {
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const text = value ?? '—';
  if (value === null || (breakdown === null && !pending) || (breakdown !== null && breakdown.total === 0)) {
    return <td className="w8-right">{text}{extra}</td>;
  }
  // 表の外にはみ出して切れないよう、画面に固定して置く (数字の左下から)
  const show = (el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    setPos({ left: Math.max(8, Math.min(r.right - 360, window.innerWidth - 376)), top: r.bottom + 4 });
  };
  const hide = () => { setPos(null); };
  return (
    <td className="w8-right talk-tipcell">
      <span
        className="talk-tipbtn" tabIndex={0} data-testid="talk-outbound-trigger" aria-describedby={pos === null ? undefined : 'talk-outbound-tip'}
        onMouseEnter={e => { show(e.currentTarget); }} onMouseLeave={hide}
        onFocus={e => { show(e.currentTarget); }} onBlur={hide}
        onKeyDown={e => { if (e.key === 'Escape') hide(); }}
      >{text}</span>
      {extra}
      {pos !== null && (
        <div id="talk-outbound-tip" className="talk-tip" role="tooltip" data-testid="talk-outbound-tip" style={{ left: pos.left, top: pos.top }}>
          {breakdown === null ? <p>内訳を集計中です</p> : (
            <>
              <table>
                <thead><tr><th>内訳</th><th className="w8-right">本数</th><th className="w8-right">発信に占める割合</th></tr></thead>
                <tbody>
                  {dialBreakdownRows(breakdown).map(r => (
                    <tr key={r.label}><td>{r.label}</td><td className="w8-right">{r.count}</td><td className="w8-right">{r.pct}</td></tr>
                  ))}
                </tbody>
              </table>
              <p className="talk-tip-note">Zoom の記録から数えた内訳（合計 {breakdown.total} 本）</p>
            </>
          )}
        </div>
      )}
    </td>
  );
}

/** 差の 1 行。cmp = 比べる期間との差、team = チームの全体との差 */
function DiffNote({ kind, cur, base, m, itemId, rules }: { kind: 'cmp' | 'team'; cur: TalkPersonRow; base: TalkPersonRow | null | undefined; m: MetricId; itemId?: string | undefined; rules: TalkSummaryResponse['rules'] }) {
  if (base === null || base === undefined) return null;
  const id = itemId ?? '';
  const d: Diff | null = metricDiff(cur, base, m, id, rules);
  if (d === null) return null;
  const head = kind === 'cmp' ? '前' : 'チーム比';
  const what = kind === 'cmp' ? '比べる期間' : base.name;
  const thin = d.faint ? '（件数が少ないので参考程度）' : '';
  return (
    <span className={diffClass(d)} data-diff={kind} data-dir={d.dir} data-faint={d.faint ? 'true' : undefined} title={`${what}: ${fmtMetric(base, m, id)}${thin}`}>{head} {d.text}</span>
  );
}

/** 行ごとの比べる相手 (undefined = 比べない、null = 相手の行が無い) */
interface RowCompare { cmp?: TalkPersonRow | null | undefined; team?: TalkPersonRow | null | undefined }

function PersonRowView({ p, checks, total, totalRow, rules, breakdownPending, onOpen, compare, select, rowId }: {
  p: TalkPersonRow; checks: TalkCheckDef[]; total: boolean; totalRow: TalkPersonRow; rules: TalkSummaryResponse['rules']; breakdownPending: boolean;
  onOpen?: (id: string) => void; compare?: RowCompare | undefined; select?: { checked: boolean; onChange: () => void } | undefined; rowId?: string;
}) {
  const apj = total ? null : judgeAppointment(p, totalRow, rules);
  const dn = (m: MetricId, itemId?: string) => (
    <>
      <DiffNote kind="cmp" cur={p} base={compare?.cmp} m={m} itemId={itemId} rules={rules} />
      {(m === 'appointmentRate' || m === 'contactRate' || m === 'proposalRate' || m === 'item') && (
        <DiffNote kind="team" cur={p} base={compare?.team} m={m} itemId={itemId} rules={rules} />
      )}
    </>
  );
  return (
    <tr className={total ? 'talk-total' : undefined} data-testid={rowId ?? (total ? 'talk-total-row' : 'talk-row')}>
      <td>
        {select !== undefined && (
          <input type="checkbox" className="talk-pick" aria-label={`${p.name}を選ぶ`} checked={select.checked} onChange={select.onChange} />
        )}
        {total || p.owner_id === null || onOpen === undefined ? p.name : (
          <button type="button" className="talk-namebtn" onClick={() => { onOpen(p.owner_id ?? ''); }}>{p.name}</button>
        )}
        {compare?.cmp === null && <span className="talk-small" data-testid="talk-no-prev">比べる期間にはデータなし</span>}
      </td>
      <td className="w8-muted">{p.team ?? '-'}</td>
      <OutboundCell value={p.outbound} breakdown={p.dial_breakdown} pending={breakdownPending} extra={dn('outbound')} />
      <td className="w8-right">{p.dials ?? '—'}{dn('dials')}</td>
      <td className="w8-right">{p.appointments}{dn('appointments')}</td>
      <Cell j={apj} className="w8-right">{fmtPct(p.appointment_rate_pct, 2)}{dn('appointmentRate')}</Cell>
      <td className="w8-right" data-testid="talk-contacts">
        {p.contacts ?? '—'}{dn('contacts')}
        {p.contact_pending > 0 && <span className="talk-small" data-testid="talk-contact-pending">接触の判定待ち {p.contact_pending} 本</span>}
      </td>
      <td className="w8-right">{fmtPct(p.contact_rate_pct, 1)}{dn('contactRate')}</td>
      <td className="w8-right">{p.date_proposal_calls ?? '—'}{dn('proposalCalls')}</td>
      <td className="w8-right">{fmtPct(p.date_proposal_rate_pct, 1)}{dn('proposalRate')}</td>
      <td className="w8-right">{fmtPerContact(p.proposals_per_contact)}{dn('perContact')}</td>
      <td className="w8-right">
        {p.reviewed}
        <span className="talk-small">対象 {p.eligible} 本 ・ 待ち {p.waiting} ・ 取れない {p.unavailable}</span>
      </td>
      {checks.map(c => {
        const it = p.items.find(i => i.id === c.id);
        const j = total ? null : judgeItem(it, totalRow.items.find(i => i.id === c.id), rules);
        return (
          <Cell key={c.id} j={j} className="w8-right">
            {fmtPct(donePct(it))}
            <span className="talk-small">{it === undefined ? '' : `できた ${String(it.applicable - it.not_done)}/${String(it.applicable)}本`}</span>
            {dn('item', c.id)}
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

/** 人の選び方・比べる相手 (無ければ従来の表) */
export interface TalkTableView {
  /** 選んだ人の担当者 ID */
  selected: string[];
  /** 選んだ人だけを表に出す */
  only: boolean;
  onToggle: (ownerId: string) => void;
  onOnly: (only: boolean) => void;
  onClear: () => void;
  /** 比べる期間の集計 (比べない・まだ読めていないときは undefined) */
  cmp?: TalkSummaryResponse | undefined;
  /** チームの全体との差を出す */
  vsTeam: boolean;
}

export function TalkSummaryTable({ data, onOpen, view }: { data: TalkSummaryResponse; onOpen: (ownerId: string) => void; view?: TalkTableView }) {
  const [sort, setSort] = useState<TalkSort | null>(null);
  const sorted = sortPeople(data.people, sort);
  const selSet = new Set(view?.selected ?? []);
  const selectedRows = data.people.filter(p => p.owner_id !== null && selSet.has(p.owner_id));
  const people = view?.only === true && selectedRows.length > 0 ? sorted.filter(p => p.owner_id !== null && selSet.has(p.owner_id)) : sorted;
  const cmpOf = (p: TalkPersonRow): TalkPersonRow | null | undefined =>
    view?.cmp === undefined ? undefined : (view.cmp.people.find(c => c.owner_id === p.owner_id) ?? null);
  const teamOf = (p: TalkPersonRow): TalkPersonRow | null | undefined =>
    view?.vsTeam !== true ? undefined : (data.team_totals.find(t => t.name === p.team) ?? null);
  const shownTeams = view?.vsTeam === true ? [...new Set(people.map(p => p.team).filter((t): t is string => t !== null))] : [];
  const selectedSum = selectedRows.length > 0 ? sumRows(selectedRows, data.checks, `選んだ人の合計（${String(selectedRows.length)}人）`) : null;
  const selectedCmp: TalkPersonRow | null | undefined = (() => {
    if (view?.cmp === undefined || selectedSum === null) return undefined;
    const rows = view.cmp.people.filter(c => c.owner_id !== null && selSet.has(c.owner_id));
    return rows.length === 0 ? null : sumRows(rows, data.checks, '比べる期間の合計');
  })();
  const toggle = (key: TalkSort['key']) => {
    setSort(prev => {
      const same = prev !== null && JSON.stringify(prev.key) === JSON.stringify(key);
      return { key, dir: same && prev.dir === 'desc' ? 'asc' : 'desc' };
    });
  };
  const ariaSort = (key: TalkSort['key']): 'ascending' | 'descending' | 'none' =>
    sort !== null && JSON.stringify(sort.key) === JSON.stringify(key) ? (sort.dir === 'asc' ? 'ascending' : 'descending') : 'none';
  const arrow = (key: TalkSort['key']): string => (ariaSort(key) === 'none' ? '' : ariaSort(key) === 'ascending' ? ' ▲' : ' ▼');
  const sortHead = (label: string, key: TalkSort['key']) => (
    <th className="w8-right" aria-sort={ariaSort(key)}>
      <button type="button" className="w8-sort" onClick={() => { toggle(key); }}>{label}{arrow(key)}</button>
    </th>
  );
  if (!data.enabled) {
    return <p className="w8-alert" role="status" data-testid="talk-disabled">まだ動いていません。トークの振り返りの設定が済むと、ここに人ごとの表が出ます。</p>;
  }
  const rules = data.rules;
  return (
    <>
      <p className="talk-legend" data-testid="talk-legend">
        <span><i className="talk-swatch red" />赤: この表の全体より {rules.item_excess_pct} ポイント以上できていない（当てはまる電話が {rules.item_min_applicable} 本以上の人だけ）</span>
        <span><i className="talk-swatch grey" />灰色: 本数が少ないので色を付けていない</span>
        <span>アポ率の赤: 架電数 {rules.appointment_min_dials} 本以上で、全体のアポ率の{ratioText(rules.appointment_rate_ratio)}未満</span>
      </p>
      {view !== undefined && (
        <p className="w8-subtle talk-selbar" data-testid="talk-selbar">
          {selectedRows.length === 0 ? (
            '名前の横のチェックで人を選ぶと、選んだ人の合計が出ます。選んだ人だけの表にもできます。'
          ) : (
            <>
              選んだ人 {selectedRows.length} 人{' '}
              <button type="button" onClick={() => { view.onOnly(!view.only); }} aria-pressed={view.only}>{view.only ? '全員を表示' : '選んだ人だけ表示'}</button>{' '}
              <button type="button" onClick={view.onClear}>選びなおす</button>
            </>
          )}
        </p>
      )}
      <div className="w8-table-wrap">
        <table className="w8-table w8-table-dense talk-table" data-testid="talk-table">
          <thead>
            <tr>
              <th colSpan={12} className="talk-group-blank" />
              {stageGroups(data.checks).map(g => <th key={g.stage + String(g.start)} colSpan={g.count} className="talk-group">{g.stage}</th>)}
            </tr>
            <tr>
              <th>名前</th>
              <th>チーム</th>
              {sortHead('発信数', { kind: 'outbound' })}
              {sortHead('架電数', { kind: 'dials' })}
              {sortHead('アポ数', { kind: 'appointments' })}
              {sortHead('アポ率', { kind: 'rate' })}
              {sortHead('接触数', { kind: 'contacts' })}
              {sortHead('接触率', { kind: 'contactRate' })}
              {sortHead('日程切り本数', { kind: 'proposalCalls' })}
              {sortHead('日程切り率', { kind: 'proposalRate' })}
              {sortHead('1接触あたりの日程切り回数', { kind: 'perContact' })}
              {sortHead('判定した電話数（接触した電話）', { kind: 'reviewed' })}
              {data.checks.map(c => {
                const key = { kind: 'item', id: c.id } as const;
                const docTip = c.baseline_pct === null ? '' : `\n資料の全体: できた ${fmtPct(100 - c.baseline_pct)}${c.baseline_basis === null ? '' : `（${c.baseline_basis}）`}`;
                return (
                  <th key={c.id} className="w8-right talk-itemhead" title={c.rule + docTip} aria-sort={ariaSort(key)}>
                    <button type="button" className="w8-sort" onClick={() => { toggle(key); }}>{c.title}{arrow(key)}</button>
                    <span className="talk-base">この表の全体 {fmtPct(donePct(data.total.items.find(i => i.id === c.id)))}</span>
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {people.map(p => (
              <PersonRowView
                key={p.owner_id ?? p.name} p={p} checks={data.checks} total={false} totalRow={data.total} rules={rules} breakdownPending={data.dial_breakdown_pending} onOpen={onOpen}
                compare={view === undefined ? undefined : { cmp: cmpOf(p), team: teamOf(p) }}
                select={view === undefined || p.owner_id === null ? undefined : { checked: selSet.has(p.owner_id), onChange: () => { view.onToggle(p.owner_id ?? ''); } }}
              />
            ))}
            {people.length === 0 && <tr><td colSpan={12 + data.checks.length} className="w8-muted">この期間に判定できた電話はまだありません。</td></tr>}
            {shownTeams.map(t => {
              const row = data.team_totals.find(x => x.name === t);
              if (row === undefined) return null;
              return (
                <PersonRowView
                  key={`team-${t}`} p={{ ...row, name: `${t}の全体` }} checks={data.checks} total totalRow={data.total} rules={rules} breakdownPending={data.dial_breakdown_pending} rowId="talk-team-row"
                  compare={view?.cmp === undefined ? undefined : { cmp: view.cmp.team_totals.find(x => x.name === t) ?? null }}
                />
              );
            })}
            {selectedSum !== null && (
              <PersonRowView p={selectedSum} checks={data.checks} total totalRow={data.total} rules={rules} breakdownPending={data.dial_breakdown_pending} rowId="talk-selected-row" compare={{ cmp: selectedCmp }} />
            )}
            <PersonRowView p={data.total} checks={data.checks} total totalRow={data.total} rules={rules} breakdownPending={data.dial_breakdown_pending} compare={view?.cmp === undefined ? undefined : { cmp: view.cmp.total }} />
          </tbody>
        </table>
      </div>
      <p className="talk-rules" data-testid="talk-rules">
        <span style={{ display: 'block' }}>各項目の割合は、その項目に当てはまる電話のうち、できた電話の割合です。</span>
        <span style={{ display: 'block' }} data-testid="talk-dials-rule">発信数は Zoom の発信すべて（相手が出る前に切ったもの・つながらなかったものを含む）、架電数はそのうちつながった通話の数で、営業KPI と同じです。アポ率 ＝ アポ数 ÷ 架電数です。</span>
        <span style={{ display: 'block' }} data-testid="talk-contact-rule">接触は、受付を越えて担当者・決裁者と話せた発信で、日程を出していなくても数えます。接触率 ＝ 接触数 ÷ 発信数。日程切り本数は、具体的な日時（週・曜日・日付）を 1 回以上出した発信の本数で、日程切り率 ＝ 日程切り本数 ÷ 発信数です。1 接触あたりの日程切り回数は、接触した電話で日時を出した回数の合計 ÷ 接触数です。つながらなかった発信は接触 0・日程切り 0 として数えます。</span>
        <span style={{ display: 'block' }} data-testid="talk-sampling">割合はすべて、判定した電話の中での割合です。判定は 1 人 1 日あたり新しい電話から {data.per_person_day} 本までで、「対象」は判定の対象になる、担当者と話せた電話（接触した電話）の本数です。電話の長さでは絞っていません。資料の全体の割合は 70 秒以上の電話で取った値で、短い電話も入るこの表とは、そのまま比べられません。</span>
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

/** 日付は「決定」を押すまで読み直さない (1 つ変えるたびに待たされないように)。期間が変わったら key で作り直す */
function DateRange({ from, to, onApply, label = '日付指定', applyLabel = '決定' }: { from: string; to: string; onApply: (from: string, to: string) => void; label?: string; applyLabel?: string }) {
  const [draftFrom, setDraftFrom] = useState(from);
  const [draftTo, setDraftTo] = useState(to);
  const ok = draftFrom !== '' && draftTo !== '' && draftFrom <= draftTo;
  const changed = draftFrom !== from || draftTo !== to;
  return (
    <form
      className="w8-subtle"
      style={{ margin: 0, display: 'inline-flex', alignItems: 'center', gap: '0.25rem' }}
      onSubmit={e => { e.preventDefault(); if (ok && changed) onApply(draftFrom, draftTo); }}
    >
      {label}{' '}
      <input type="date" aria-label={label === '日付指定' ? '開始日' : `${label}の開始日`} value={draftFrom} max={draftTo || undefined} onChange={e => { setDraftFrom(e.target.value); }} />
      {' 〜 '}
      <input type="date" aria-label={label === '日付指定' ? '終了日' : `${label}の終了日`} value={draftTo} min={draftFrom || undefined} onChange={e => { setDraftTo(e.target.value); }} />
      <button type="submit" disabled={!ok || !changed}>{applyLabel}</button>
    </form>
  );
}

function Controls({ route, navigate, teams, from, to, today }: {
  route: TalkRoute; navigate: (r: AdminRoute) => void; teams: string[]; from: string; to: string; today: string;
}) {
  const active = activePeriod(from, to, today);
  const prev = previousPeriod(from, to);
  const cmpOn = validCompare(route.cmp_from, route.cmp_to);
  const go = (patch: Partial<TalkRoute>) => {
    const next = { ...route, owner: '', call: '', ...patch };
    // 「直前の同じ長さ」を選んでいたときは、期間を変えても直前に追従する
    if (('from' in patch || 'to' in patch) && !('cmp_from' in patch) && cmpOn && route.cmp_from === prev.from && route.cmp_to === prev.to) {
      const np = previousPeriod(next.from, next.to === '' ? next.from : next.to);
      next.cmp_from = np.from;
      next.cmp_to = np.to;
    }
    navigate(next);
  };
  return (
    <>
      <div className="talk-bar">
        <div role="group" aria-label="期間" className="talk-period">
          {PERIOD_LABELS.map(([k, label]) => (
            <button key={k} type="button" aria-pressed={active === k} onClick={() => { go(periodRange(k, today)); }}>{label}</button>
          ))}
        </div>
        <DateRange key={from + to} from={from} to={to} onApply={(f, t) => { go({ from: f, to: t }); }} />
        <label className="w8-subtle" style={{ margin: 0 }}>
          チーム{' '}
          <select value={route.team} aria-label="チームで絞り込む" onChange={e => { go({ team: e.target.value }); }}>
            <option value="">すべて</option>
            {teams.map(t => <option key={t} value={t}>{t}</option>)}
          </select>
        </label>
      </div>
      <details className="talk-more" open={cmpOn || route.vs_team === '1' ? true : undefined} data-testid="talk-compare-ctl">
        <summary>比べる</summary>
        <div className="talk-bar">
          <div role="group" aria-label="比べる期間" className="talk-period">
            <button type="button" aria-pressed={!cmpOn} onClick={() => { go({ cmp_from: '', cmp_to: '' }); }}>比べない</button>
            <button type="button" aria-pressed={cmpOn && route.cmp_from === prev.from && route.cmp_to === prev.to} onClick={() => { go({ cmp_from: prev.from, cmp_to: prev.to }); }}>直前の同じ長さの期間</button>
          </div>
          <DateRange key={`c${route.cmp_from}${route.cmp_to}`} label="比べる期間" applyLabel="比べる期間を決定" from={cmpOn ? route.cmp_from : prev.from} to={cmpOn ? route.cmp_to : prev.to} onApply={(f, t) => { go({ cmp_from: f, cmp_to: t }); }} />
          <label className="w8-subtle" style={{ margin: 0 }}>
            <input type="checkbox" checked={route.vs_team === '1'} onChange={e => { go({ vs_team: e.target.checked ? '1' : '' }); }} />{' '}
            チームの全体との差を出す
          </label>
        </div>
      </details>
    </>
  );
}

/** 表に、人の選び方と比べる期間 (もう 1 回 summary を読む) をつなぐ */
function SummaryWithCompare({ d, route, navigate, fx, teamQ }: { d: TalkSummaryResponse; route: TalkRoute; navigate: (r: AdminRoute) => void; fx: boolean; teamQ: string }) {
  const cmpOn = validCompare(route.cmp_from, route.cmp_to);
  const cmpPath = cmpOn && !fx ? `/api/crm/talk-review/summary?from=${encodeURIComponent(route.cmp_from)}&to=${encodeURIComponent(route.cmp_to)}${teamQ}` : null;
  const live = useTalkGet<TalkSummaryResponse>(cmpPath);
  const cmp: TalkState<TalkSummaryResponse> = cmpOn && fx ? { status: 'ok', data: fixtureSummary(route.cmp_from, route.cmp_to, route.team, true) } : live;
  const selected = parsePeople(route.people);
  const go = (patch: Partial<TalkRoute>) => { navigate({ ...route, owner: '', call: '', ...patch }); };
  const cmpData = cmp.status === 'ok' && cmp.data.enabled ? cmp.data : undefined;
  return (
    <>
      {cmpOn && (
        <p className="w8-subtle" data-testid="talk-cmp-note">
          {route.cmp_from === route.cmp_to ? route.cmp_from : `${route.cmp_from} 〜 ${route.cmp_to}`} と比べています。各数字の下の「前」は、その期間との差です（割合はポイント、本数は増減）。
          {cmp.status === 'loading' && ' 比べる期間の数字を読んでいます…'}
          {(cmp.status === 'counting') && ' 比べる期間を集計しています…'}
          {cmpData !== undefined && cmpData.counting_days.length > 0 && ' 比べる期間のアポは数えている途中です。'}
        </p>
      )}
      {cmpOn && (cmp.status === 'error' || cmp.status === 'not_configured') && (
        <p className="w8-alert" role="alert" data-testid="talk-cmp-error">比べる期間の数字を読めませんでした。期間を短くするか、しばらくしてからやり直してください。</p>
      )}
      {route.vs_team === '1' && (
        <p className="w8-subtle" data-testid="talk-vsteam-note">「チーム比」は、その人のチーム全体の割合との差です（ポイント）。チーム全体の割合は、そのチームの人の件数を合わせて出しています。件数が少ない割合の差は薄く出しています。</p>
      )}
      <TalkSummaryTable
        data={d}
        onOpen={id => { go({ owner: id }); }}
        view={{
          selected, only: route.only === '1',
          onToggle: id => { go({ people: joinPeople(togglePerson(selected, id)) }); },
          onOnly: v => { go({ only: v ? '1' : '' }); },
          onClear: () => { go({ people: '', only: '' }); },
          cmp: cmpData, vsTeam: route.vs_team === '1',
        }}
      />
    </>
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
                {describeDays(d.counting_days)} のアポは数えている途中です。数え終わるまで、その日のアポは表に入っていません（数え終わると自動で更新します）。
              </p>
            )}
            {d.dials_error && (
              <p className="w8-alert" role="alert" data-testid="talk-dials-error">
                架電数を読めませんでした（営業KPI のシートを読めないため）。架電数とアポ率は出していません。しばらくしてから開き直してください。
              </p>
            )}
            {d.dials_partial && !d.dials_error && (
              <p className="w8-subtle" role="status" data-testid="talk-dials-partial">
                {d.dials_fetched_at === null ? '最終日の架電数は途中の数です。' : `最終日の架電数は ${d.dials_fetched_at} 時点の数です。`}
              </p>
            )}
            <SummaryWithCompare d={d} route={withPeriod} navigate={navigate} fx={fx} teamQ={teamQ} />
          </>
        )}
      </Loader>
    </>
  );
}
