/**
 * 案件（取引）でまとめる数字。画面に依存しない計算だけを置く。
 * 日付は JST の暦日 YYYY-MM-DD。応募日別の件数 (byDate) が無い求人は null として扱い、0 とは区別する。
 * 課金は重なる期間・金額なしの行があると足さず、分からないものは null のまま返す。
 */
import { addDays, billingOverlaps, countApplications, daysBetween } from './timelineModel';
import type { BillingEntry } from './timelineModel';

export const WINDOW_DAYS = 28;

export interface DealListingInput {
  id: string;
  title: string;
  media: 'hrh' | 'airwork';
  /** 掲載終了。終了した求人は、掲載していた間の応募と課金を合計に含める。 */
  ended: boolean;
  /** 応募日別の件数。取得できていないときは null。 */
  byDate: Record<string, number> | null;
  /** この求人の課金 (課金CSV)。取り込みが無ければ空。 */
  billing: readonly BillingEntry[];
}

export interface ListingNumbers {
  id: string;
  recent: number | null;
  previous: number | null;
  /** 直近4週の期間にかかる課金 (日割り)。分からないときは null。 */
  billing: number | null;
  perApplication: number | null;
}

/** [asOf の 28 日前, asOf] が直近4週、その前の 28 日が前の4週。asOf は今日。 */
export function windows(asOf: string): { recentStart: string; previousStart: string; end: string } {
  return { recentStart: addDays(asOf, -(WINDOW_DAYS - 1)), previousStart: addDays(asOf, -(2 * WINDOW_DAYS - 1)), end: addDays(asOf, 1) };
}

/** 期間 [start, end] (両端を含む) のうち、[from, toExclusive) と重なる日数。 */
function overlapDays(start: string, end: string, from: string, toExclusive: string): number {
  const first = start > from ? start : from;
  const lastExclusive = addDays(end, 1) < toExclusive ? addDays(end, 1) : toExclusive;
  return Math.max(0, daysBetween(first, lastExclusive));
}

/** from〜toExclusive にかかった課金 (期間の金額を日割り)。足せない・金額なし・課金データなしは null。 */
export function billingInRange(entries: readonly BillingEntry[], from: string, toExclusive: string): number | null {
  if (!entries.length || billingOverlaps(entries)) return null;
  let total = 0;
  for (const entry of entries) {
    const days = overlapDays(entry.start, entry.end, from, toExclusive);
    if (days === 0) continue;
    if (entry.amountYen === null) return null;
    total += entry.amountYen * days / (daysBetween(entry.start, entry.end) + 1);
  }
  return Math.round(total);
}

export function listingNumbers(listing: DealListingInput, asOf: string): ListingNumbers {
  const { recentStart, previousStart, end } = windows(asOf);
  const recent = listing.byDate ? countApplications(listing.byDate, recentStart, end) : null;
  const previous = listing.byDate ? countApplications(listing.byDate, previousStart, recentStart) : null;
  const billing = billingInRange(listing.billing, recentStart, end);
  return { id: listing.id, recent, previous, billing, perApplication: billing !== null && recent ? Math.round(billing / recent) : null };
}

export interface DealTotals {
  recent: number | null;
  previous: number | null;
  /** 応募日が分からず合計に入っていない求人の数。 */
  unknownApplications: number;
  billing: number | null;
  /** 課金が分からず合計に入っていない、掲載中の求人の数。 */
  unknownBilling: number;
  /** 課金の合計 ÷ 応募の合計。どちらも分かる求人だけで割る。 */
  perApplication: number | null;
  listings: number;
  live: number;
  ended: number;
}

export function dealTotals(listings: readonly DealListingInput[], asOf: string): DealTotals {
  const numbers = listings.map(listing => listingNumbers(listing, asOf));
  const known = numbers.filter(n => n.recent !== null);
  const sum = (values: (number | null)[]) => values.reduce<number>((total, value) => total + (value ?? 0), 0);
  const billed = numbers.filter(n => n.billing !== null);
  const bothKnown = numbers.filter(n => n.billing !== null && n.recent !== null);
  const billingOfBoth = sum(bothKnown.map(n => n.billing));
  const applicationsOfBoth = sum(bothKnown.map(n => n.recent));
  return {
    recent: known.length ? sum(known.map(n => n.recent)) : null,
    previous: known.length ? sum(known.map(n => n.previous)) : null,
    unknownApplications: numbers.length - known.length,
    billing: billed.length ? sum(billed.map(n => n.billing)) : null,
    unknownBilling: listings.filter((listing, index) => !listing.ended && numbers[index]?.billing === null).length,
    perApplication: bothKnown.length && applicationsOfBoth > 0 ? Math.round(billingOfBoth / applicationsOfBoth) : null,
    listings: listings.length,
    live: listings.filter(listing => !listing.ended).length,
    ended: listings.filter(listing => listing.ended).length,
  };
}

/** 案件の応募に占める、この求人の割合 (%)。案件の合計が 0 または不明なら null。 */
export function shareOfDeal(listingRecent: number | null, dealRecent: number | null): number | null {
  if (listingRecent === null || !dealRecent) return null;
  return Math.round(listingRecent / dealRecent * 100);
}

/** 月曜はじまりの週の初日。 */
export function weekStart(date: string): string {
  const weekday = (new Date(`${date}T00:00:00Z`).getUTCDay() + 6) % 7;
  return addDays(date, -weekday);
}

export interface WeeklySeries {
  weeks: string[];
  /** 求人ごとの週ごとの応募。応募日が未取得の求人は counts が null。 */
  perListing: { id: string; counts: number[] | null }[];
  /** 求人の合計の週ごとの応募 (応募日が分かる求人だけ)。 */
  total: number[];
  /** 週ごとの課金の合計 (日割り、課金が分かる求人だけ)。課金が一件も分からなければ null。 */
  billing: number[] | null;
}

export function weeklySeries(listings: readonly DealListingInput[], asOf: string, weekCount = 12): WeeklySeries {
  const last = weekStart(asOf);
  const weeks = Array.from({ length: weekCount }, (_, i) => addDays(last, -7 * (weekCount - 1 - i)));
  const perListing = listings.map(listing => ({
    id: listing.id,
    counts: listing.byDate ? weeks.map(week => countApplications(listing.byDate ?? undefined, week, addDays(week, 7))) : null,
  }));
  const total = weeks.map((_, i) => perListing.reduce((sum, row) => sum + (row.counts?.[i] ?? 0), 0));
  const billedListings = listings.filter(listing => listing.billing.length && !billingOverlaps(listing.billing) && listing.billing.every(entry => entry.amountYen !== null));
  const billing = billedListings.length ? weeks.map(week => billedListings.reduce((sum, listing) => sum + (billingInRange(listing.billing, week, addDays(week, 7)) ?? 0), 0)) : null;
  return { weeks, perListing, total, billing };
}

export interface DealEvent {
  date: string;
  kind: 'billing_start' | 'billing_end' | 'body';
  listingId: string;
  text: string;
  /** その日の前の4週・後の4週の、案件の応募の合計。分からない・4週たっていないときは null。 */
  before: number | null;
  after: number | null;
}

/** 案件の応募の合計 (応募日が分かる求人だけ) の [from, toExclusive)。 */
export function dealApplicationsBetween(listings: readonly DealListingInput[], from: string, toExclusive: string): number | null {
  const known = listings.filter(listing => listing.byDate);
  if (!known.length) return null;
  return known.reduce((sum, listing) => sum + countApplications(listing.byDate ?? undefined, from, toExclusive), 0);
}

export interface EventSource { listingId: string; bodyChangedAt: string | null }

/** 新しい順。課金は期間の開始・終了、本文は最後に変えた日。画像の変更は版を読み込んでいないため含めない。 */
export function dealEvents(listings: readonly DealListingInput[], sources: readonly EventSource[], asOf: string, titleOf: (id: string) => string, formatYen: (value: number) => string): DealEvent[] {
  const events: Omit<DealEvent, 'before' | 'after'>[] = [];
  for (const listing of listings) {
    const title = titleOf(listing.id);
    const entries = [...listing.billing].sort((a, b) => a.start.localeCompare(b.start));
    entries.forEach((entry, index) => {
      const amount = entry.amountYen === null ? '金額なし' : formatYen(entry.amountYen);
      const previous = entries[index - 1];
      const changed = previous?.amountYen !== entry.amountYen;
      if (changed) events.push({ date: entry.start, kind: 'billing_start', listingId: listing.id, text: `${title}：課金が${previous ? '変わりました' : '始まりました'}（${amount}）` });
      const next = entries[index + 1];
      if (next?.start !== addDays(entry.end, 1)) events.push({ date: entry.end, kind: 'billing_end', listingId: listing.id, text: `${title}：課金の期間が終わりました` });
    });
  }
  for (const source of sources) {
    if (!source.bodyChangedAt) continue;
    events.push({ date: source.bodyChangedAt, kind: 'body', listingId: source.listingId, text: `${titleOf(source.listingId)}：本文を変えました` });
  }
  return events.sort((a, b) => b.date.localeCompare(a.date)).map(event => {
    const afterEnd = addDays(event.date, WINDOW_DAYS);
    const complete = afterEnd <= addDays(asOf, 1);
    return {
      ...event,
      before: dealApplicationsBetween(listings, addDays(event.date, -WINDOW_DAYS), event.date),
      after: complete ? dealApplicationsBetween(listings, event.date, afterEnd) : null,
    };
  });
}

export interface MediaCutoff { media: 'hrh' | 'airwork'; lastApplication: string | null; lastBilling: string | null }

/** 媒体ごとの、応募と課金がどの日まであるか。 */
export function mediaCutoffs(listings: readonly DealListingInput[]): MediaCutoff[] {
  return (['hrh', 'airwork'] as const).filter(media => listings.some(listing => listing.media === media)).map(media => {
    const own = listings.filter(listing => listing.media === media);
    const dates = own.flatMap(listing => Object.entries(listing.byDate ?? {}).filter(([, count]) => count > 0).map(([date]) => date));
    const ends = own.flatMap(listing => listing.billing.map(entry => entry.end));
    return { media, lastApplication: dates.length ? dates.reduce((a, b) => a > b ? a : b) : null, lastBilling: ends.length ? ends.reduce((a, b) => a > b ? a : b) : null };
  });
}
