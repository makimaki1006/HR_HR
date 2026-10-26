// トークの振り返り: 期間の計算・並べ替え・表示用の整形・架空データ。画面と試験で共有する。
import type { TalkCallDetailResponse } from '../../generated/TalkCallDetailResponse';
import type { TalkCallRow } from '../../generated/TalkCallRow';
import type { TalkCallsResponse } from '../../generated/TalkCallsResponse';
import type { DialBreakdown } from '../../generated/DialBreakdown';
import type { TalkCheckDef } from '../../generated/TalkCheckDef';
import type { TalkItemRate } from '../../generated/TalkItemRate';
import type { TalkPersonRow } from '../../generated/TalkPersonRow';
import type { TalkSummaryResponse } from '../../generated/TalkSummaryResponse';

// ------------------------------------------------------------ 期間 (JST、週は月曜始まり)

const DAY_MS = 86_400_000;
const ymdFmt = new Intl.DateTimeFormat('en-CA', { timeZone: 'Asia/Tokyo', year: 'numeric', month: '2-digit', day: '2-digit' });

/** 日本時間の今日 (YYYY-MM-DD) */
export function jstToday(now: Date = new Date()): string {
  return ymdFmt.format(now);
}

function toUtc(ymd: string): number {
  const [y, m, d] = ymd.split('-').map(Number);
  return Date.UTC(y ?? 1970, (m ?? 1) - 1, d ?? 1);
}

export function addDays(ymd: string, n: number): string {
  return new Date(toUtc(ymd) + n * DAY_MS).toISOString().slice(0, 10);
}

/** その日の週の月曜 */
export function mondayOf(ymd: string): string {
  const dow = new Date(toUtc(ymd)).getUTCDay(); // 0=日
  return addDays(ymd, -((dow + 6) % 7));
}

export type PeriodKey = 'today' | 'yesterday' | 'thisWeek' | 'lastWeek';
export const PERIOD_LABELS: [PeriodKey, string][] = [
  ['today', '今日'],
  ['yesterday', '昨日'],
  ['thisWeek', '今週'],
  ['lastWeek', '先週'],
];

export function periodRange(key: PeriodKey, today: string): { from: string; to: string } {
  switch (key) {
    case 'today':
      return { from: today, to: today };
    case 'yesterday': {
      const y = addDays(today, -1);
      return { from: y, to: y };
    }
    case 'thisWeek':
      return { from: mondayOf(today), to: today };
    case 'lastWeek': {
      const m = addDays(mondayOf(today), -7);
      return { from: m, to: addDays(m, 6) };
    }
  }
}

/** from/to がどの切り替えに当たるか。当たらなければ日付指定 */
export function activePeriod(from: string, to: string, today: string): PeriodKey | 'custom' {
  for (const [k] of PERIOD_LABELS) {
    const r = periodRange(k, today);
    if (r.from === from && r.to === to) return k;
  }
  return 'custom';
}

// ------------------------------------------------------------ 表示用の整形

/** 1 接触あたりの日程切り回数 (小数 1 桁)。値が無ければ「-」 */
export function fmtPerContact(v: number | null): string {
  return v === null ? '-' : v.toFixed(1);
}

/** 発信の内訳の表の行。社内の内線は 0 本なら出さない。割合は発信 (内訳の合計) に占める % を小数 1 桁 (四捨五入) */
export function dialBreakdownRows(b: DialBreakdown): { label: string; count: number; pct: string }[] {
  const rows: [string, number][] = [
    ['つながった', b.connected],
    ['発信に失敗した（相手側の回線・番号の問題など）', b.failed],
    ['こちらで切った', b.cancelled],
    ['モニタリング（他の人の通話を聞いた）', b.monitor],
  ];
  if (b.extension > 0) rows.push(['社内の内線', b.extension]);
  return rows.map(([label, count]) => ({ label, count, pct: b.total === 0 ? '-' : `${(Math.round((count / b.total) * 1000) / 10).toFixed(1)}%` }));
}

export function fmtPct(v: number | null, digits = 0): string {
  return v === null ? '-' : `${v.toFixed(digits)}%`;
}

const dtFmt = new Intl.DateTimeFormat('ja-JP', { timeZone: 'Asia/Tokyo', month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false });
/** RFC 3339 を日本時間の「10/19 10:12」に */
export function fmtCallTime(rfc3339: string): string {
  const d = new Date(rfc3339);
  if (Number.isNaN(d.getTime())) return rfc3339;
  const p = Object.fromEntries(dtFmt.formatToParts(d).map(x => [x.type, x.value]));
  return `${p.month ?? ''}/${p.day ?? ''} ${p.hour ?? ''}:${p.minute ?? ''}`;
}

export function fmtSeconds(sec: number): string {
  const m = Math.floor(sec / 60);
  return m > 0 ? `${String(m)}分${String(sec % 60)}秒` : `${String(sec)}秒`;
}

export const CALL_STATUS_TEXT: Record<string, string> = {
  waiting: '文字起こし待ち',
  unavailable: '録音が残っていない',
  failed: '判定できなかった',
};

export const MARK_TEXT: Record<string, string> = { done: '○', not_done: '×', na: '−' };
export const MARK_LABEL: Record<string, string> = { done: 'できている', not_done: 'できていない', na: '該当なし' };

/** 数えている途中の日 (YYYY-MM-DD) を「10/06〜10/08、10/10」に。続く日はまとめる */
export function describeDays(days: string[]): string {
  const dayMs = (d: string): number => Date.parse(`${d}T00:00:00Z`);
  const md = (d: string): string => d.slice(5).replace('-', '/');
  const sorted = [...days].sort();
  const groups: string[][] = [];
  for (const d of sorted) {
    const g = groups.at(-1);
    const prev = g?.at(-1);
    if (g !== undefined && prev !== undefined && dayMs(d) - dayMs(prev) === 86_400_000) g.push(d);
    else groups.push([d]);
  }
  return groups.map(g => (g.length === 1 ? md(g[0] ?? '') : `${md(g[0] ?? '')}〜${md(g.at(-1) ?? '')}`)).join('、');
}

// ------------------------------------------------------------ 並べ替え

export type TalkSortKey =
  | { kind: 'rate' } | { kind: 'outbound' } | { kind: 'dials' } | { kind: 'appointments' } | { kind: 'reviewed' }
  | { kind: 'contacts' } | { kind: 'contactRate' } | { kind: 'proposalCalls' } | { kind: 'proposalRate' } | { kind: 'perContact' }
  | { kind: 'item'; id: string };
export interface TalkSort { key: TalkSortKey; dir: 'asc' | 'desc' }

/** できた割合 (%)。当てはまる電話が 0 本なら null */
export function donePct(it: Pick<TalkItemRate, 'not_done' | 'applicable'> | undefined): number | null {
  if (it === undefined || it.applicable <= 0) return null;
  return ((it.applicable - it.not_done) / it.applicable) * 100;
}

function sortValue(p: TalkPersonRow, key: TalkSortKey): number | null {
  switch (key.kind) {
    case 'rate': return p.appointment_rate_pct;
    case 'outbound': return p.outbound;
    case 'dials': return p.dials;
    case 'appointments': return p.appointments;
    case 'reviewed': return p.reviewed;
    case 'contacts': return p.contacts;
    case 'contactRate': return p.contact_rate_pct;
    case 'proposalCalls': return p.date_proposal_calls;
    case 'proposalRate': return p.date_proposal_rate_pct;
    case 'perContact': return p.proposals_per_contact;
    case 'item': return donePct(p.items.find(i => i.id === key.id));
  }
}

// ------------------------------------------------------------ 色の決まり (画面に出ている数字だけで決める)

export type CellState = 'red' | 'none' | 'grey' | 'empty';
export interface CellJudgement { state: CellState; reason: string }
const EPS = 1e-9;
const pt0 = (v: number): string => String(Math.round(v));

/** 項目のセル: 赤 / 色なし / 灰色。total はこの表の全体の行の同じ項目 */
export function judgeItem(it: TalkItemRate | undefined, total: TalkItemRate | undefined, rules: TalkSummaryResponse['rules']): CellJudgement {
  const done = donePct(it);
  if (it === undefined || done === null) return { state: 'empty', reason: '当てはまる電話がありません。' };
  const doneN = it.applicable - it.not_done;
  const head = `できた ${String(doneN)}/${String(it.applicable)}本（${pt0(done)}%）。`;
  if (it.applicable < rules.item_min_applicable) {
    return { state: 'grey', reason: `当てはまる電話が ${String(it.applicable)} 本で、${String(rules.item_min_applicable)} 本未満なので色を付けていません。` };
  }
  const base = donePct(total);
  if (base === null) return { state: 'none', reason: `${head}この表の全体と比べられません。` };
  const diff = base - done;
  if (diff >= rules.item_excess_pct - EPS) {
    return { state: 'red', reason: `${head}この表の全体 ${pt0(base)}% より ${pt0(diff)} ポイント低いので赤。` };
  }
  return { state: 'none', reason: `${head}この表の全体との差が ${String(rules.item_excess_pct)} ポイント未満。` };
}

/** アポ率のセル。total はこの表の全体の行 */
export function judgeAppointment(p: TalkPersonRow, total: TalkPersonRow, rules: TalkSummaryResponse['rules']): CellJudgement {
  const r = p.appointment_rate_pct;
  if (p.dials === null || r === null) return { state: 'empty', reason: '架電数が分からないのでアポ率を出せません。' };
  if (p.dials < rules.appointment_min_dials) {
    return { state: 'grey', reason: `架電数が ${String(p.dials)} 本で、${String(rules.appointment_min_dials)} 本未満なので色を付けていません。` };
  }
  const base = total.appointment_rate_pct;
  if (base === null) return { state: 'none', reason: 'この表の全体と比べられません。' };
  const limit = base * rules.appointment_rate_ratio;
  if (r < limit - EPS) {
    return { state: 'red', reason: `アポ率 ${r.toFixed(2)}% が、この表の全体 ${base.toFixed(2)}% の${ratioText(rules.appointment_rate_ratio)}（${limit.toFixed(2)}%）未満なので赤。` };
  }
  return { state: 'none', reason: `アポ率 ${r.toFixed(2)}% は、この表の全体 ${base.toFixed(2)}% の${ratioText(rules.appointment_rate_ratio)}以上。` };
}

export function ratioText(ratio: number): string {
  return ratio === 0.5 ? '半分' : `${String(Math.round(ratio * 100))}%`;
}

/** 値の無い行は向きによらず最後に置く */
export function sortPeople(people: TalkPersonRow[], sort: TalkSort | null): TalkPersonRow[] {
  if (sort === null) return people;
  const sign = sort.dir === 'asc' ? 1 : -1;
  return [...people].sort((a, b) => {
    const x = sortValue(a, sort.key);
    const y = sortValue(b, sort.key);
    if (x === null && y === null) return 0;
    if (x === null) return 1;
    if (y === null) return -1;
    return (x - y) * sign;
  });
}

// ------------------------------------------------------------ 架空データ (mode=fixture と試験)

export const FIXTURE_CHECKS: TalkCheckDef[] = [
  { id: "reception_name", stage: "受付", title: "受付で担当者を名前で呼ぶ", rule: "受付(担当者以外)が出たとき、営業が担当者を「〇〇様」と名前で呼んだか。名前が分からず「採用のご担当者様」と聞いた場合は、つながった後に名前を聞いていればできたとする。", baseline_pct: null, baseline_basis: null },
  { id: "no_demand_question", stage: "日付の前", title: "日付の前に需要を聞かない", rule: "最初に日付(週・曜日・日)を出すより前に、営業が「今は募集していないですか」「充足していますか」「採用の予定はありますか」など、募集・需要の有無を聞いたら、できていない。", baseline_pct: 10, baseline_basis: "話せた電話のうち" },
  { id: "early_date", stage: "日付の前", title: "早く日付を出す", rule: "担当者が出て営業が名乗ってから、営業の発言2〜3回以内に、具体的な週・曜日・日(「来週の水曜か木曜あたり」など)を出したか。「お話しする機会をいただけませんか」のような日付なしの依頼は日付を出したことにしない。", baseline_pct: 81, baseline_basis: "話せた電話のうち" },
  { id: "no_content_before_date", stage: "日付の前", title: "日程を切るとき「サービスの中身」「〇〇職の件」と言わない", rule: "最初に日程を切る一言、またはその直前に、「採用サービスの中身をお伝えしたく」「〇〇職の採用の件で」など、中身・用件を営業から言ったら、できていない。先方に「何の件ですか」と聞かれて答えた場合は対象外(できたとする)。", baseline_pct: 42, baseline_basis: "日程を切った電話のうち" },
  { id: "kizonkan", stage: "日付の前", title: "既存感(前回のこと・改めた理由・覚えていますか)", rule: "日付の前に、①前回の電話に触れる(「前回〇月にお電話した」)②改めた理由を添える(「タイミングが合わなかったので」)③覚えているかを確かめる、のうち①と②があればできた。どれもなければできていない。", baseline_pct: null, baseline_basis: null },
  { id: "what_is_it_handled", stage: "日付の前", title: "「何の件ですか」と聞かれたら1〜2文で答えて日付に戻る", rule: "先方に「何の件ですか」「覚えていない」と聞かれたとき、営業が1〜2文で用件を答え、すぐ日付を出し直したらできた。説明を長く続けて日付に戻らなければできていない。", baseline_pct: null, baseline_basis: null },
  { id: "busy_reoffer", stage: "断られた後", title: "「忙しい」「その日は無理」で引かない", rule: "日付に対して「忙しい」「その日は無理」など予定の理由で断られたとき、営業がそのまま「また改めます」と引き下がったら、できていない。別の日を出し直したか、聞き返したならこの項目はできた。", baseline_pct: 11, baseline_basis: "「忙しい」と言われた電話のうち" },
  { id: "busy_concrete_day", stage: "断られた後", title: "「忙しい」の後に具体的な日を出す", rule: "予定の理由で断られた後、営業がこちらから具体的な別の週・曜日・日を出したらできた。「いつ頃ならよろしいですか」と先方に選ばせた、または引き下がった場合はできていない。", baseline_pct: 67, baseline_basis: "「忙しい」と言われた電話のうち" },
  { id: "demand_denied_short", stage: "断られた後", title: "「募集していない」と言われたら短く終える", rule: "先方が需要を否定した(「今は募集していない」「充足している」)後、営業が「本当にないですか」と何度も聞き直したらできていない。次の時期・担当者名を聞いて短く終えたらできた。", baseline_pct: null, baseline_basis: null },
];

export const FIXTURE_RULES: TalkSummaryResponse['rules'] = {
  item_min_applicable: 5,
  item_excess_pct: 20,
  appointment_min_dials: 100,
  appointment_rate_ratio: 0.5,
};

const rate = (id: string, nd: number, ap: number, flagged = false): TalkItemRate => ({ id, not_done: nd, applicable: ap, rate_pct: ap === 0 ? null : (nd / ap) * 100, flagged });

// 項目ごとの [できていない本数, 該当した本数] を 花子・二郎・三郎・四郎 の順に
const FIXTURE_COUNTS: Record<string, [number, number][]> = {
  reception_name: [[6, 38], [10, 40], [5, 30], [2, 9]],
  no_demand_question: [[3, 40], [14, 45], [4, 30], [1, 10]],
  early_date: [[20, 40], [36, 45], [10, 30], [5, 10]],
  no_content_before_date: [[8, 25], [12, 28], [9, 20], [3, 6]],
  kizonkan: [[12, 40], [20, 45], [9, 30], [4, 10]],
  what_is_it_handled: [[2, 8], [6, 12], [1, 6], [0, 3]],
  busy_reoffer: [[1, 9], [4, 10], [1, 8], [1, 2]],
  busy_concrete_day: [[6, 9], [7, 10], [3, 8], [2, 2]],
  demand_denied_short: [[2, 7], [3, 9], [6, 7], [1, 2]],
};

function person(idx: number, id: string, name: string, team: string, dials: number, appts: number, flagged: boolean, reviewed: number, waiting: number, unavailable: number, contact: [number, number, number, number]): TalkPersonRow {
  const outbound = Math.round(dials * 2.3);
  const connectedN = Math.round(outbound * 0.8);
  const cancelled = Math.round(outbound * 0.1);
  const monitor = idx === 1 ? 4 : 0;
  const extension = idx === 0 ? 2 : 0;
  const failed = outbound - connectedN - cancelled - monitor - extension;
  const dial_breakdown: DialBreakdown = { connected: connectedN, failed, cancelled, monitor, extension, total: outbound };
  const [contacts, proposalCalls, proposals, pending] = contact;
  const items = FIXTURE_CHECKS.map(c => {
    const [nd, ap] = FIXTURE_COUNTS[c.id]?.[idx] ?? [0, 0];
    const all = FIXTURE_COUNTS[c.id] ?? [];
    const tnd = all.reduce((a, x) => a + x[0], 0);
    const tap = all.reduce((a, x) => a + x[1], 0);
    const hit = ap >= FIXTURE_RULES.item_min_applicable && tap > 0 && (nd / ap) * 100 >= (tnd / tap) * 100 + FIXTURE_RULES.item_excess_pct;
    return rate(c.id, nd, ap, hit);
  });
  return { owner_id: id, name, team, outbound, dial_breakdown, dials, appointments: appts, appointment_rate_pct: dials === 0 ? null : (appts / dials) * 100, appointment_rate_flagged: flagged, contacts, contact_rate_pct: (contacts / outbound) * 100, date_proposal_calls: proposalCalls, date_proposal_rate_pct: (proposalCalls / outbound) * 100, proposals_per_contact: contacts === 0 ? null : proposals / contacts, contact_pending: pending, eligible: reviewed + waiting + unavailable + Math.round(reviewed / 2), reviewed, waiting, unavailable, items };
}

export const FIXTURE_PEOPLE: TalkPersonRow[] = [
  person(0, '9001', '架空 花子', '架空BPOチーム', 220, 5, false, 60, 12, 8, [150, 60, 96, 4]),
  person(1, '9002', 'サンプル 二郎', '架空BPOチーム', 310, 2, true, 70, 6, 4, [190, 90, 120, 0]),
  person(2, '9003', 'ダミー 三郎', '架空営業チーム', 150, 4, false, 45, 3, 2, [100, 45, 80, 2]),
  person(3, '9004', '架空 四郎', '架空営業チーム', 90, 1, false, 12, 20, 6, [40, 10, 12, 0]),
];

function totalOf(people: TalkPersonRow[], checks: TalkCheckDef[]): TalkPersonRow {
  const sum = (f: (p: TalkPersonRow) => number) => people.reduce((a, p) => a + f(p), 0);
  const dials = sum(p => p.dials ?? 0);
  const outbound = sum(p => p.outbound ?? 0);
  const appts = sum(p => p.appointments);
  const contacts = sum(p => p.contacts ?? 0);
  const proposalCalls = sum(p => p.date_proposal_calls ?? 0);
  const proposals = people.reduce((a, p) => a + (p.proposals_per_contact ?? 0) * (p.contacts ?? 0), 0);
  return {
    owner_id: null, name: 'この表の全体', team: null, outbound, dial_breakdown: people.reduce<DialBreakdown>((a, p) => {
      const b = p.dial_breakdown;
      return b === null ? a : { connected: a.connected + b.connected, failed: a.failed + b.failed, cancelled: a.cancelled + b.cancelled, monitor: a.monitor + b.monitor, extension: a.extension + b.extension, total: a.total + b.total };
    }, { connected: 0, failed: 0, cancelled: 0, monitor: 0, extension: 0, total: 0 }), dials, appointments: appts,
    appointment_rate_pct: dials === 0 ? null : (appts / dials) * 100, appointment_rate_flagged: false,
    contacts, contact_rate_pct: outbound === 0 ? null : (contacts / outbound) * 100,
    date_proposal_calls: proposalCalls, date_proposal_rate_pct: outbound === 0 ? null : (proposalCalls / outbound) * 100,
    proposals_per_contact: contacts === 0 ? null : proposals / contacts, contact_pending: sum(p => p.contact_pending),
    eligible: sum(p => p.eligible), reviewed: sum(p => p.reviewed), waiting: sum(p => p.waiting), unavailable: sum(p => p.unavailable),
    items: checks.map(c => {
      const nd = sum(p => p.items.find(i => i.id === c.id)?.not_done ?? 0);
      const ap = sum(p => p.items.find(i => i.id === c.id)?.applicable ?? 0);
      return rate(c.id, nd, ap);
    }),
  };
}

export function fixtureSummary(from: string, to: string, team: string, prior = false): TalkSummaryResponse {
  const base = FIXTURE_PEOPLE.filter(p => team === '' || p.team === team);
  const people = prior ? base.filter(p => p.owner_id !== '9004').map(priorOf) : base;
  const teamNames = [...new Set(people.map(p => p.team).filter((t): t is string => t !== null))];
  const team_totals = teamNames.map(t => ({ ...totalOf(people.filter(p => p.team === t), FIXTURE_CHECKS), name: t, team: t }));
  return {
    from, to, as_of: `${to}T14:05:00+09:00`, enabled: true,
    teams: ['架空BPOチーム', '架空営業チーム'],
    checks: FIXTURE_CHECKS, rules: FIXTURE_RULES, people, per_person_day: 10, counting_days: [], dials_error: false, dials_partial: true, dials_fetched_at: `${to} 06:30`, dial_breakdown_pending: false, total: totalOf(people, FIXTURE_CHECKS), team_totals,
  };
}

/** 比べる期間の架空の数字: 発信・接触が少なく、できていない電話が少し少ない。四郎は前の期間に行が無い */
function priorOf(p: TalkPersonRow): TalkPersonRow {
  const outbound = p.outbound === null ? null : Math.round(p.outbound * 0.9);
  const dials = p.dials === null ? null : Math.round(p.dials * 0.9);
  const appointments = Math.max(0, p.appointments - 1);
  const contacts = p.contacts === null ? null : Math.round(p.contacts * 0.8);
  const proposalCalls = p.date_proposal_calls === null ? null : Math.round(p.date_proposal_calls * 0.85);
  return {
    ...p, outbound, dials, appointments, contacts, date_proposal_calls: proposalCalls,
    dial_breakdown: null,
    appointment_rate_pct: dials === null || dials === 0 ? null : (appointments / dials) * 100,
    contact_rate_pct: outbound === null || outbound === 0 || contacts === null ? null : (contacts / outbound) * 100,
    date_proposal_rate_pct: outbound === null || outbound === 0 || proposalCalls === null ? null : (proposalCalls / outbound) * 100,
    items: p.items.map(i => rate(i.id, i.not_done + 2 > i.applicable ? i.applicable : i.not_done + 2, i.applicable)),
  };
}

/** 'd' できた / 'x' できていない / '-' 該当なし を項目の順に */
const marks = (s: string): TalkCallRow['items'] => FIXTURE_CHECKS.map((c, i) => ({ id: c.id, status: { d: 'done', x: 'not_done' }[s[i] ?? '-'] ?? 'na' }));

export function fixtureCalls(ownerId: string, from: string, to: string): TalkCallsResponse {
  const name = FIXTURE_PEOPLE.find(p => p.owner_id === ownerId)?.name ?? '架空 担当者';
  const mk = (n: number, at: string, sec: number, status: string, outcome: string | null, m: string, summary: string | null): TalkCallRow =>
    ({ hubspot_call_id: `fx-${ownerId}-${String(n)}`, call_at: `${to}T${at}:00+09:00`, duration_sec: sec, status, outcome, items: status === 'judged' ? marks(m) : [], summary });
  return {
    owner_id: ownerId, name, from, to, checks: FIXTURE_CHECKS,
    calls: [
      mk(1, '15:42', 212, 'judged', '日程が決まらなかった', 'dxxxd-xx-', '日程の前に「今は募集していないですか」と聞き、「忙しい」と言われて引き下がっています。'),
      mk(2, '14:20', 95, 'judged', '日程が決まった', 'dddddd---', null),
      mk(3, '13:05', 48, 'judged', '担当者不在', 'x--------', '受付で担当者を名前で呼ばず、用件だけを伝えています。'),
      mk(4, '11:30', 130, 'waiting', null, '', null),
      mk(5, '10:12', 0, 'unavailable', null, '', null),
      mk(6, '09:48', 77, 'failed', null, '', null),
    ],
  };
}

const fallbackCall: TalkCallRow = { hubspot_call_id: 'fx-9001-1', call_at: '2026-10-19T15:42:00+09:00', duration_sec: 0, status: 'waiting', outcome: null, items: [], summary: null };

export function fixtureDetail(callId: string): TalkCallDetailResponse {
  const m = /^fx-(.+)-(\d+)$/.exec(callId);
  const ownerId = m?.[1] ?? '9001';
  const n = Number(m?.[2] ?? '1');
  const call = fixtureCalls(ownerId, '2026-10-19', '2026-10-19').calls.find(c => c.hubspot_call_id === callId)
    ?? fixtureCalls(ownerId, '2026-10-19', '2026-10-19').calls[0] ?? fallbackCall;
  const rep = FIXTURE_PEOPLE.find(p => p.owner_id === ownerId)?.name ?? '架空 担当者';
  const base = { call, owner_id: ownerId, rep_name: rep, checks: FIXTURE_CHECKS, reached_person: true, reception: true, first_date_turn: null, hubspot_url: `https://app.hubspot.com/contacts/0/record/0-48/${callId}`, model: '架空モデル', checks_version: 'fixture' };
  const it = (id: string, status: string, evidence: number[], quote: string | null, advice: string | null, note: string | null = null) => ({ id, status, evidence, quote, advice, note });
  const A: Record<string, string> = {"reception_name": "「〇〇様いらっしゃいますか」と名前で呼ぶ。名前が分からないときだけ「採用のご担当者様」と聞き、つながったら名前を聞いて記録する。", "no_demand_question": "需要には触れずに日付を出す。冒頭で聞くと「していない」という断り文句をこちらから言わせることになる。", "early_date": "前回のことに一言触れ、改めた理由を添えて、すぐ「来週の水曜か木曜あたりはいかがですか」と日付を出す。", "no_content_before_date": "中身・用件は言わず、前回のことと改めた理由を一言添えて日付を出す。用件は聞かれたら1〜2文で答えて日付に戻る。", "kizonkan": "「覚えていただけてますか? 前回〇月にお電話したんですけど、タイミングが合わなかったので」と、前回のことと改めた理由をセットで言う。", "what_is_it_handled": "「採用のご支援の件です。求人媒体の選び方から運用までをまとめてお手伝いしています」と1〜2文で答え、すぐ日付を出す。", "busy_reoffer": "「そうですよね、失礼しました。では再来週の火曜か水曜あたりはいかがですか」と別の週・日をもう1回出す。", "busy_concrete_day": "「いつ頃なら」と聞き返さず、こちらから別の日を出す。", "demand_denied_short": "「次に採用を考えられるのはどのあたりになりそうですか」と次の時期だけ聞いて、丁寧に短く終える。"};
  const items: TalkCallDetailResponse['items'] = [
    it('reception_name', 'done', [2], '総務の佐藤様はいらっしゃいますか。', null),
    it('no_demand_question', 'not_done', [5], '今は採用を募集されていないですか。', A.no_demand_question ?? null),
    it('early_date', 'not_done', [7], '一度お時間をいただけませんか。', A.early_date ?? null, '日付(週・曜日)を出していない'),
    it('no_content_before_date', 'not_done', [7], '採用サービスの中身をお伝えしたく', A.no_content_before_date ?? null),
    it('kizonkan', 'done', [4], '以前お電話した、架空エージェントの者です。前回はタイミングが合わなかったので', null),
    it('what_is_it_handled', 'na', [], null, null, '「何の件ですか」と聞かれていない'),
    it('busy_reoffer', 'not_done', [9], 'では、また改めてお電話します。', A.busy_reoffer ?? null),
    it('busy_concrete_day', 'not_done', [9], 'では、また改めてお電話します。', A.busy_concrete_day ?? null),
    it('demand_denied_short', 'na', [], null, null, '需要を否定されていない'),
  ];
  if (n === 2) return { ...base, items, transcript: null, transcript_error: '録音が残っていません' };
  if (call.status !== 'judged') return { ...base, items: [], transcript: null, transcript_error: '判定前のため文字起こしを取っていません' };
  return {
    ...base, items, transcript_error: null,
    transcript: [
      { n: 1, speaker: 'customer', text: 'はい、架空商事です。' },
      { n: 2, speaker: 'rep', text: '総務の佐藤様はいらっしゃいますか。' },
      { n: 3, speaker: 'customer', text: '少々お待ちください。……はい、佐藤です。' },
      { n: 4, speaker: 'rep', text: '以前お電話した、架空エージェントの者です。前回はタイミングが合わなかったので、改めてお電話しました。' },
      { n: 5, speaker: 'rep', text: '今は採用を募集されていないですか。' },
      { n: 6, speaker: 'customer', text: 'いえ、募集はしていますけど。' },
      { n: 7, speaker: 'rep', text: '採用サービスの中身をお伝えしたく、一度お時間をいただけませんか。' },
      { n: 8, speaker: 'customer', text: 'うーん、今月はちょっと忙しくて。' },
      { n: 9, speaker: 'rep', text: 'そうですか、では、また改めてお電話します。' },
      { n: 10, speaker: 'customer', text: 'はい、お願いします。' },
    ],
  };
}
