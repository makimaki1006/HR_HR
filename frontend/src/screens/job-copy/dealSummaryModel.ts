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
  /** この求人の応募データを取得した日 (JST)。分からないときは null。 */
  fetchedDate: string | null;
  /** 課金データを集めていない媒体 (AirWork)。課金の合計・1件あたりから外す。 */
  billingUnavailable?: boolean;
  /** ほかの取引にも結び付いている求人。 */
  inOtherDeals?: boolean;
}

/** 複数の求人に結び付いた応募の「余分な写し」の数を日ごとに持つ。案件の合計から引く。 */
export type Overlap = Readonly<Record<string, number>>;
function overlapIn(overlap: Overlap, from: string, toExclusive: string): number {
  return countApplications(overlap, from, toExclusive);
}

/**
 * 案件の「直近4週」の終わりの日。応募データがある求人の取得日のうち、いちばん早い日
 * (全部の求人のデータがそろっている最後の日)。ひとつも無ければ null。
 * 求人の取得日は record.overallApplications.fetchedAt の JST の日 (#185 の帯と同じ定義)。
 */
export function dealCutoff(listings: readonly DealListingInput[]): string | null {
  const dates = listings.filter(listing => listing.byDate && listing.fetchedDate).map(listing => listing.fetchedDate ?? '');
  return dates.length ? dates.reduce((a, b) => a < b ? a : b) : null;
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
  const billing = listing.billingUnavailable ? null : billingInRange(listing.billing, recentStart, end);
  return { id: listing.id, recent, previous, billing, perApplication: billing !== null && recent ? Math.round(billing / recent) : null };
}

export interface DealTotals {
  recent: number | null;
  previous: number | null;
  /** 応募日が分からず合計に入っていない求人の数。 */
  unknownApplications: number;
  billing: number | null;
  /** 課金が分からない求人の数 (終了した求人を含む)。ひとつでもあれば課金の合計は出さない。 */
  unknownBilling: number;
  /** 課金の合計 ÷ 応募の合計。全部の求人の課金と応募が分かるときだけ出す。 */
  perApplication: number | null;
  listings: number;
  live: number;
  ended: number;
  /** 課金データがないため、課金の合計・1件あたりに入れていない求人の数 (AirWork)。 */
  excludedBilling: number;
}

export function dealTotals(listings: readonly DealListingInput[], asOf: string, overlap: Overlap = {}): DealTotals {
  const { recentStart, previousStart, end } = windows(asOf);
  const numbers = listings.map(listing => listingNumbers(listing, asOf));
  const known = numbers.filter(n => n.recent !== null);
  const sum = (values: (number | null)[]) => values.reduce<number>((total, value) => total + (value ?? 0), 0);
  const recent = known.length ? sum(known.map(n => n.recent)) - overlapIn(overlap, recentStart, end) : null;
  const billed = listings.map((listing, index) => ({ listing, n: numbers[index] })).filter(item => !item.listing.billingUnavailable);
  const excludedBilling = listings.length - billed.length;
  const unknownBilling = billed.filter(item => item.n?.billing === null).length;
  const unknownApplications = numbers.length - known.length;
  const billedUnknownApps = billed.filter(item => item.n?.recent === null).length;
  const billing = billed.length && unknownBilling === 0 ? sum(billed.map(item => item.n?.billing ?? null)) : null;
  // 課金のない求人があるときは、課金のある求人だけの応募で割る (重なる応募の引き算は求人を絞れないので使わない)。
  const billedRecent = excludedBilling === 0 ? recent : billedUnknownApps === billed.length ? null : sum(billed.map(item => item.n?.recent ?? null));
  return {
    recent,
    previous: known.length ? sum(known.map(n => n.previous)) - overlapIn(overlap, previousStart, recentStart) : null,
    unknownApplications,
    billing,
    unknownBilling,
    perApplication: billing !== null && billedRecent && billedUnknownApps === 0 ? Math.round(billing / billedRecent) : null,
    listings: listings.length,
    live: listings.filter(listing => !listing.ended).length,
    ended: listings.filter(listing => listing.ended).length,
    excludedBilling,
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
  /** 案件の合計の週ごとの応募 (応募日が分かる求人だけ。重なる応募は1回だけ)。 */
  total: number[];
  /** 週ごとの課金の合計 (日割り)。課金が分からない求人がひとつでもあれば null。 */
  billing: number[] | null;
}

export function weeklySeries(listings: readonly DealListingInput[], asOf: string, overlap: Overlap = {}, weekCount = 12): WeeklySeries {
  const last = weekStart(asOf);
  const weeks = Array.from({ length: weekCount }, (_, i) => addDays(last, -7 * (weekCount - 1 - i)));
  const perListing = listings.map(listing => ({
    id: listing.id,
    counts: listing.byDate ? weeks.map(week => countApplications(listing.byDate ?? undefined, week, addDays(week, 7))) : null,
  }));
  const total = weeks.map((week, i) => perListing.reduce((sum, row) => sum + (row.counts?.[i] ?? 0), 0) - overlapIn(overlap, week, addDays(week, 7)));
  const billedListings = listings.filter(listing => !listing.billingUnavailable);
  const cells = weeks.map(week => billedListings.map(listing => billingInRange(listing.billing, week, addDays(week, 7))));
  const billing = billedListings.length > 0 && cells.every(row => row.every(cell => cell !== null)) ? cells.map(row => row.reduce<number>((sum, cell) => sum + cell, 0)) : null;
  return { weeks, perListing, total, billing };
}

export interface DealEvent {
  date: string;
  kind: 'billing_start' | 'billing_end' | 'body';
  listingId: string;
  text: string;
  /** その日の前の4週・後の4週の、案件の応募の合計。分からない・4週たっていないときは null。 */
  before: number | null;
  /** 前の4週が、応募データの最初の日より前にかかっていて数えられない。 */
  beforeIncomplete: boolean;
  after: number | null;
}

/** 案件の応募の合計 (応募日が分かる求人だけ) の [from, toExclusive)。 */
export function dealApplicationsBetween(listings: readonly DealListingInput[], from: string, toExclusive: string, overlap: Overlap = {}): number | null {
  const known = listings.filter(listing => listing.byDate);
  if (!known.length) return null;
  return known.reduce((sum, listing) => sum + countApplications(listing.byDate ?? undefined, from, toExclusive), 0) - overlapIn(overlap, from, toExclusive);
}

/** 応募データの最初の日 (どの求人でもいちばん古い応募の日)。 */
export function firstDataDate(listings: readonly DealListingInput[]): string | null {
  const dates = listings.flatMap(listing => Object.entries(listing.byDate ?? {}).filter(([, count]) => count > 0).map(([date]) => date));
  return dates.length ? dates.reduce((a, b) => a < b ? a : b) : null;
}

/**
 * 課金の出来事。週ごとの実績は1本の流れとして見る。
 * 出すのは「課金が始まった週（初めて費用が出た週、止まったあとに再開した週）」と
 * 「費用0の週が2週以上続いた、その最初の週」だけ。週ごとの費用の上下と、データの最後より後の空白は出来事にしない。
 */
export function billingEvents(entries: readonly BillingEntry[]): { date: string; kind: 'start' | 'stop'; amountYen: number }[] {
  const sorted = [...entries].sort((a, b) => a.start.localeCompare(b.start));
  const out: { date: string; kind: 'start' | 'stop'; amountYen: number }[] = [];
  let running = false;
  sorted.forEach((entry, i) => {
    if (entry.amountYen !== null && entry.amountYen > 0) {
      if (!running) out.push({ date: entry.start, kind: 'start', amountYen: entry.amountYen });
      running = true;
    } else if (entry.amountYen === 0 && running && sorted[i + 1]?.amountYen === 0) {
      out.push({ date: entry.start, kind: 'stop', amountYen: 0 });
      running = false;
    }
  });
  return out;
}

/** 帯の上に出す短い金額 (例 94,286 → 9.4万)。0・不明は空。 */
export const shortYen = (value: number): string => !Number.isFinite(value) || value <= 0 ? '' : value >= 10000 ? `${(Math.round(value / 1000) / 10).toFixed(1)}万` : `${String(Math.round(value))}円`;

export interface EventSource { listingId: string; bodyChangedAt: string | null }

/** 新しい順。課金は期間の開始・終了、本文は最後に変えた日。画像の変更は版を読み込んでいないため含めない。 */
export function dealEvents(listings: readonly DealListingInput[], sources: readonly EventSource[], asOf: string, titleOf: (id: string) => string, formatYen: (value: number) => string, overlap: Overlap = {}): DealEvent[] {
  const events: Omit<DealEvent, 'before' | 'after' | 'beforeIncomplete'>[] = [];
  const first = firstDataDate(listings);
  for (const listing of listings) {
    const title = titleOf(listing.id);
    for (const event of billingEvents(listing.billing)) {
      events.push(event.kind === 'start'
        ? { date: event.date, kind: 'billing_start', listingId: listing.id, text: `${title}：課金が始まりました（${formatYen(event.amountYen)}）` }
        : { date: event.date, kind: 'billing_end', listingId: listing.id, text: `${title}：課金が止まりました` });
    }
  }
  for (const source of sources) {
    if (!source.bodyChangedAt) continue;
    events.push({ date: source.bodyChangedAt, kind: 'body', listingId: source.listingId, text: `${titleOf(source.listingId)}：本文を変えました` });
  }
  return events.sort((a, b) => b.date.localeCompare(a.date)).map(event => {
    const afterEnd = addDays(event.date, WINDOW_DAYS);
    const complete = afterEnd <= addDays(asOf, 1);
    const beforeStart = addDays(event.date, -WINDOW_DAYS);
    const beforeIncomplete = first === null || beforeStart < first;
    return {
      ...event,
      before: beforeIncomplete ? null : dealApplicationsBetween(listings, beforeStart, event.date, overlap),
      beforeIncomplete,
      after: complete ? dealApplicationsBetween(listings, event.date, afterEnd, overlap) : null,
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

/** 課金の週ごとの行 (HRハッカー実績) を、日割りの計算に使う期間に直す。 */
export function perfBillingEntries(perf: { rows: readonly { period_start: string; period_end: string; cost_yen: number | null }[] } | undefined, mediaJobId: string): BillingEntry[] {
  return (perf?.rows ?? []).map(row => ({ source: 'hrhacker' as const, start: row.period_start, end: row.period_end, amountYen: row.cost_yen, taxIncluded: null, media: 'HRハッカー', mediaJobId }));
}

/** 事業所（会社）の集計で、どの求人を何回数えるかを決める1か所の規則。 */
export const ROLLUP_RULES = {
  /** 複数の取引に結び付いた求人: 取引の集計ではどの取引でも全部数え、事業所の集計では1回だけ数える。 */
  multiDeal: '取引の集計ではそれぞれ全部数え、事業所の集計では1回だけ数える',
  /** 集計する期間は絞らない。 */
  period: '契約期間では絞らず、取得できている全期間',
  /** 事業所 = 取引に結び付いた会社。 */
  company: '事業所は、取引に結び付いた会社',
  /** 取引が多い事業所は先頭だけ集計する。 */
  maxDeals: 10,
} as const;

export interface DealGroup { id: string; name: string; inputs: DealListingInput[]; overlap: Overlap }

/** 事業所の集計に使う求人 (同じ求人は1回だけ) と、重なる応募 (日ごとに、取引のうち最大のもの)。 */
export function companyRollup(groups: readonly DealGroup[]): { inputs: DealListingInput[]; overlap: Overlap } {
  const byId = new Map<string, DealListingInput>();
  for (const group of groups) for (const input of group.inputs) if (!byId.has(input.id)) byId.set(input.id, input);
  const overlap: Record<string, number> = {};
  for (const group of groups) for (const [day, value] of Object.entries(group.overlap)) overlap[day] = Math.max(overlap[day] ?? 0, value);
  return { inputs: [...byId.values()], overlap };
}
