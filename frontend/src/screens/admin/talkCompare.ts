// トークの振り返り: 選んだ人の合計・期間どうしの差・チームの全体との差。画面と試験で共有する純粋な計算。
import type { DialBreakdown } from '../../generated/DialBreakdown';
import type { TalkCheckDef } from '../../generated/TalkCheckDef';
import type { TalkItemRate } from '../../generated/TalkItemRate';
import type { TalkPersonRow } from '../../generated/TalkPersonRow';
import type { TalkSummaryResponse } from '../../generated/TalkSummaryResponse';
import { addDays, donePct } from './talkModel';

// ------------------------------------------------------------ URL の値

/** people=1,2,3 を ID の並びに (空・重複を除く) */
export function parsePeople(s: string): string[] {
  const out: string[] = [];
  for (const x of s.split(',')) {
    const id = x.trim();
    if (id !== '' && !out.includes(id)) out.push(id);
  }
  return out;
}

export function joinPeople(ids: string[]): string {
  return ids.join(',');
}

/** チェックの入れ替え */
export function togglePerson(ids: string[], id: string): string[] {
  return ids.includes(id) ? ids.filter(x => x !== id) : [...ids, id];
}

// ------------------------------------------------------------ 比べる期間

function dayCount(from: string, to: string): number {
  return Math.round((Date.parse(`${to}T00:00:00Z`) - Date.parse(`${from}T00:00:00Z`)) / 86_400_000) + 1;
}

/** 直前の同じ長さの期間 (例: 10/12〜10/18 の直前は 10/05〜10/11) */
export function previousPeriod(from: string, to: string): { from: string; to: string } {
  const n = dayCount(from, to);
  const pTo = addDays(from, -1);
  return { from: addDays(pTo, -(n - 1)), to: pTo };
}

/** 比べる期間として使えるか (両方あり、順序が正しい) */
export function validCompare(cmpFrom: string, cmpTo: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(cmpFrom) && /^\d{4}-\d{2}-\d{2}$/.test(cmpTo) && cmpFrom <= cmpTo;
}

// ------------------------------------------------------------ 選んだ人の合計

function sumNullable(rows: TalkPersonRow[], f: (p: TalkPersonRow) => number | null): number | null {
  let any = false;
  let s = 0;
  for (const p of rows) {
    const v = f(p);
    if (v !== null) { any = true; s += v; }
  }
  return any ? s : null;
}

const pct = (n: number | null, d: number | null): number | null => (n === null || d === null || d <= 0 ? null : (n / d) * 100);

/**
 * 選んだ人の合計の行。率は件数の合計から出す (表の全体の行と同じ数え方)。
 * アポ数は人ごとの数の足し算。同じ取引を 2 人に付けた分は、足すと 2 回数える。
 */
export function sumRows(rows: TalkPersonRow[], checks: TalkCheckDef[], name: string): TalkPersonRow {
  const outbound = sumNullable(rows, p => p.outbound);
  const dials = sumNullable(rows, p => p.dials);
  const appointments = rows.reduce((a, p) => a + p.appointments, 0);
  const contacts = sumNullable(rows, p => p.contacts);
  const proposalCalls = sumNullable(rows, p => p.date_proposal_calls);
  // 1 接触あたりの日程切り回数: 人ごとの値を接触数で戻して足し、合計の接触数で割る
  const proposals = rows.reduce((a, p) => a + (p.proposals_per_contact ?? 0) * (p.contacts ?? 0), 0);
  let breakdown: DialBreakdown | null = null;
  for (const p of rows) {
    const b = p.dial_breakdown;
    if (b === null) continue;
    breakdown = breakdown === null ? { ...b } : {
      connected: breakdown.connected + b.connected, failed: breakdown.failed + b.failed, cancelled: breakdown.cancelled + b.cancelled,
      monitor: breakdown.monitor + b.monitor, extension: breakdown.extension + b.extension, total: breakdown.total + b.total,
    };
  }
  const items: TalkItemRate[] = checks.map(c => {
    let nd = 0;
    let ap = 0;
    for (const p of rows) {
      const it = p.items.find(i => i.id === c.id);
      if (it !== undefined) { nd += it.not_done; ap += it.applicable; }
    }
    return { id: c.id, not_done: nd, applicable: ap, rate_pct: ap === 0 ? null : (nd / ap) * 100, flagged: false };
  });
  return {
    owner_id: null, name, team: null,
    outbound, dial_breakdown: breakdown, dials, appointments,
    appointment_rate_pct: pct(appointments, dials), appointment_rate_flagged: false,
    contacts, contact_rate_pct: pct(contacts, outbound),
    date_proposal_calls: proposalCalls, date_proposal_rate_pct: pct(proposalCalls, outbound),
    proposals_per_contact: contacts === null || contacts === 0 ? null : proposals / contacts,
    contact_pending: rows.reduce((a, p) => a + p.contact_pending, 0),
    eligible: rows.reduce((a, p) => a + p.eligible, 0), reviewed: rows.reduce((a, p) => a + p.reviewed, 0),
    waiting: rows.reduce((a, p) => a + p.waiting, 0), unavailable: rows.reduce((a, p) => a + p.unavailable, 0),
    items,
  };
}

// ------------------------------------------------------------ 差

export type MetricId =
  | 'outbound' | 'dials' | 'appointments' | 'appointmentRate' | 'contacts' | 'contactRate'
  | 'proposalCalls' | 'proposalRate' | 'perContact' | 'item';

export interface Diff {
  /** 「+3.2pt」「−4」など */
  text: string;
  dir: 'up' | 'down' | 'flat';
  /** 割合の分母が小さく、差を大きく見せない (色を付けない) */
  faint: boolean;
  /** 色を付けてよい (できた率・アポ率・接触率・日程切り率だけ。どれも高いほど良い) */
  colored: boolean;
}

const MINUS = '−';
function signed(v: number, digits: number, unit: string): string {
  const r = Number(v.toFixed(digits));
  if (r === 0) return `±0${unit}`;
  return `${r > 0 ? '+' : MINUS}${Math.abs(r).toFixed(digits)}${unit}`;
}

const RATE: MetricId[] = ['appointmentRate', 'contactRate', 'proposalRate', 'item'];

function valueOf(p: TalkPersonRow, m: MetricId, itemId: string): number | null {
  switch (m) {
    case 'outbound': return p.outbound;
    case 'dials': return p.dials;
    case 'appointments': return p.appointments;
    case 'appointmentRate': return p.appointment_rate_pct;
    case 'contacts': return p.contacts;
    case 'contactRate': return p.contact_rate_pct;
    case 'proposalCalls': return p.date_proposal_calls;
    case 'proposalRate': return p.date_proposal_rate_pct;
    case 'perContact': return p.proposals_per_contact;
    case 'item': return donePct(p.items.find(i => i.id === itemId));
  }
}

/** 割合の分母が小さいか。どちらかの側が小さければ差を薄く出す (既存の色付けの下限と同じ数) */
function isThin(a: TalkPersonRow, b: TalkPersonRow, m: MetricId, itemId: string, rules: TalkSummaryResponse['rules']): boolean {
  switch (m) {
    case 'appointmentRate': return (a.dials ?? 0) < rules.appointment_min_dials || (b.dials ?? 0) < rules.appointment_min_dials;
    case 'contactRate':
    case 'proposalRate': return (a.outbound ?? 0) < rules.appointment_min_dials || (b.outbound ?? 0) < rules.appointment_min_dials;
    case 'item': {
      const x = a.items.find(i => i.id === itemId)?.applicable ?? 0;
      const y = b.items.find(i => i.id === itemId)?.applicable ?? 0;
      return x < rules.item_min_applicable || y < rules.item_min_applicable;
    }
    default: return false;
  }
}

/** cur と base の差 (cur − base)。どちらかの値が無ければ null */
export function metricDiff(cur: TalkPersonRow, base: TalkPersonRow, m: MetricId, itemId: string, rules: TalkSummaryResponse['rules']): Diff | null {
  const x = valueOf(cur, m, itemId);
  const y = valueOf(base, m, itemId);
  if (x === null || y === null) return null;
  const d = x - y;
  const isRate = RATE.includes(m);
  const digits = m === 'perContact' ? 2 : isRate ? 1 : 0;
  const flat = Number(d.toFixed(digits)) === 0;
  return { text: signed(d, digits, isRate ? 'pt' : ''), dir: flat ? 'flat' : d > 0 ? 'up' : 'down', faint: isRate && isThin(cur, base, m, itemId, rules), colored: isRate };
}

/** セルに出す値の文字 (比べる期間の値を、マウスを当てたときに見せる) */
export function fmtMetric(p: TalkPersonRow, m: MetricId, itemId: string): string {
  const v = valueOf(p, m, itemId);
  if (v === null) return '-';
  if (RATE.includes(m)) return `${v.toFixed(m === 'item' ? 0 : 1)}%`;
  return m === 'perContact' ? v.toFixed(1) : String(v);
}

/** 差の色の名前。良くなった = good、悪くなった = bad (どの割合も高いほど良い) */
export function diffClass(d: Diff): string {
  if (d.faint) return 'talk-diff talk-diff-faint';
  if (!d.colored || d.dir === 'flat') return 'talk-diff';
  return d.dir === 'up' ? 'talk-diff talk-diff-good' : 'talk-diff talk-diff-bad';
}
