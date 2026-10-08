/**
 * Dummy billing (仮の課金データ). Real billing data is not available yet, so the 課金 lane, the
 * period table and the overview billing total show a made-up amount, in the demo and for real jobs.
 *
 * Rules (2026-10-08):
 * - Everything from here is labelled 「仮の課金データ（ダミー）」 wherever it is shown.
 * - It is never added to a real amount (HRハッカー cost_yen or a billing CSV row). A real row
 *   replaces the dummy for the days it covers; the dummy keeps only the other days.
 * - The amounts are the same every time for the same job (seeded by the job id).
 * - Set DUMMY_BILLING_ENABLED to false to remove it everywhere once real billing data is connected.
 */
import type { BillingEntry } from './timelineModel';

/** The single switch. false: no dummy billing anywhere on the screen. */
export const DUMMY_BILLING_ENABLED = true;
/** Shown next to every dummy amount. */
export const DUMMY_BILLING_LABEL = '仮の課金データ（ダミー）';
export const DUMMY_BILLING_NOTE = `${DUMMY_BILLING_LABEL}は、実際の課金データがまだ無いため表示している架空の金額です。実際の請求額ではありません。実際の課金データがある日は、そちらを表示します。`;

const DAY_MS = 86_400_000;
const dayNumber = (date: string) => Math.floor(Date.parse(`${date}T00:00:00Z`) / DAY_MS);
const fromDay = (day: number) => new Date(day * DAY_MS).toISOString().slice(0, 10);

/** FNV-1a: the same job id always gives the same seed. */
function seedOf(text: string): number {
  let hash = 0x811c9dc5;
  for (const char of text) {
    hash ^= char.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash;
}
/** mulberry32: a small deterministic random sequence from the seed. */
function sequence(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let value = state;
    value = Math.imul(value ^ (value >>> 15), value | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
    return ((value ^ (value >>> 14)) >>> 0) / 4_294_967_296;
  };
}

/**
 * One dummy period per calendar month between start and end (inclusive days), clipped to them.
 * A full month costs 20,000–80,000円 (in 1,000円 steps); a clipped month is prorated by days.
 */
export function dummyBillingEntries(jobId: string, media: string, start: string, end: string): BillingEntry[] {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(start) || !/^\d{4}-\d{2}-\d{2}$/.test(end) || start > end) return [];
  const random = sequence(seedOf(jobId));
  const entries: BillingEntry[] = [];
  let month = start.slice(0, 7);
  while (`${month}-01` <= end && entries.length < 120) {
    const monthStart = `${month}-01`;
    const year = Number(month.slice(0, 4)); const value = Number(month.slice(5, 7));
    const next = `${String(value === 12 ? year + 1 : year).padStart(4, '0')}-${String(value === 12 ? 1 : value + 1).padStart(2, '0')}`;
    const monthEnd = fromDay(dayNumber(`${next}-01`) - 1);
    const monthly = 20_000 + Math.floor(random() * 61) * 1_000;
    const from = monthStart < start ? start : monthStart;
    const to = monthEnd > end ? end : monthEnd;
    const fullDays = dayNumber(monthEnd) - dayNumber(monthStart) + 1;
    const days = dayNumber(to) - dayNumber(from) + 1;
    entries.push({ source: 'dummy', start: from, end: to, amountYen: Math.round(monthly * days / fullDays), taxIncluded: null, media, plan: DUMMY_BILLING_LABEL });
    month = next;
  }
  return entries;
}

/**
 * Removes from the dummy periods every day a real billing row covers. A dummy period cut into
 * pieces keeps its amount per day (prorated), so a real row never adds to a dummy amount.
 */
export function withoutRealDays(dummy: readonly BillingEntry[], real: readonly BillingEntry[]): BillingEntry[] {
  const covered = real.map(entry => [dayNumber(entry.start), dayNumber(entry.end)] as const).sort((a, b) => a[0] - b[0]);
  const result: BillingEntry[] = [];
  for (const entry of dummy) {
    const first = dayNumber(entry.start); const last = dayNumber(entry.end);
    const total = last - first + 1;
    let cursor = first;
    const pieces: [number, number][] = [];
    for (const [from, to] of covered) {
      if (to < cursor || from > last) continue;
      if (from > cursor) pieces.push([cursor, from - 1]);
      cursor = Math.max(cursor, to + 1);
      if (cursor > last) break;
    }
    if (cursor <= last) pieces.push([cursor, last]);
    for (const [from, to] of pieces) {
      const days = to - from + 1;
      result.push({ ...entry, start: fromDay(from), end: fromDay(to), amountYen: entry.amountYen === null ? null : Math.round(entry.amountYen * days / total) });
    }
  }
  return result;
}

export function isDummyBilling(entry: Pick<BillingEntry, 'source'>): boolean {
  return entry.source === 'dummy';
}
