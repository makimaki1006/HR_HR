/**
 * Cross-job overview: one row per job on a shared calendar. Shows the latest change found between
 * two acquisitions (取得日A〜取得日B) and the applications per day in the N days before A and after B.
 * A, B and the days between them are left out: which content was shown then is not known. Rows are
 * sortable but never ranked as winners; the numbers sit side by side and do not say why
 * applications changed.
 */
import type { JobCopyRecord } from './data';
import {
  addDays, applicationBuckets, asOfDate, billingConflict, billingEntries, billingOverlaps, boundaryStatus, buildPeriods, changeKinds, countApplications, countRanges, daysBetween, dummyBilling,
  MIN_RATE_DAYS, multiListingByDate, realBilling, versionChanges,
} from './timelineModel';
import type { ApplicationBucket, BillingEntry, CountRange, TimelinePeriod } from './timelineModel';
import { overviewReasonText } from './reasonCategories';

export const OVERVIEW_WINDOW_DAYS = 14;
/** A before/after window shorter than this is not compared (the same rule as the period table). */
export { MIN_RATE_DAYS };
/** True when the window is long enough to put its rate beside others. */
export function comparableRate(rate: WindowRate | null): boolean {
  return rate !== null && rate.days >= MIN_RATE_DAYS && rate.perDay !== null;
}

export interface WindowRate {
  /**
   * Days actually covered. Only days whose content is known count (timelineModel.countRanges): the
   * before-window ends the day before 取得日A and the after-window starts the day after 取得日B,
   * because the change may fall on either acquisition day, before or after that day's
   * acquisition. The after-window stops at the last day the later content is known to be shown or
   * at the day counts were taken.
   */
  days: number;
  applications: number;
  perDay: number | null;
}
/**
 * A change found between two acquisitions. from: 取得日A (the earlier content was seen), to: 取得日B
 * (the later content was seen). For media publication times from = to = the publication day.
 */
export interface ChangeWindow { from: string; to: string; exact: boolean }
export interface OverviewRow {
  jobId: string;
  title: string;
  company: string;
  media: string;
  /** The latest version that changed something; null when no change was found. */
  lastChange: ChangeWindow | null;
  kinds: ('給与' | '本文' | '画像')[];
  /** Every change found (for the mini calendar). */
  changes: ChangeWindow[];
  /** Start day of the first version; null without a dated version. */
  firstDate: string | null;
  before: WindowRate | null;
  after: WindowRate | null;
  /** Applications recorded in HubSpot with a date, by week, for the mini calendar. */
  weeks: ApplicationBucket[];
  /** Real billing total; null when no real billing amount is known (not zero). Dummy amounts are never in it. */
  billingYen: number | null;
  billingConnected: boolean;
  billingMissingAmount: boolean;
  /** Two billing periods share days; the total is left blank instead of adding them up. */
  billingOverlapping: boolean;
  /** An HRハッカー実績 row and a billing CSV row cover the same days; neither is chosen. */
  billingConflict: boolean;
  /** The real billing rows are the demo's made-up HRハッカー amounts. */
  billingFictional: boolean;
  /** The dummy billing (仮の課金データ) is shown for this job. It is never added up or sorted. */
  hasDummyBilling: boolean;
  /** The two most frequent application reasons with counts and n, 「記録なし」, or 「未取得」. */
  reasonText: string;
  /** No application data at all (different from zero applications). */
  applicationsAvailable: boolean;
  asOf: string;
}

function rate(job: JobCopyRecord, start: string, endExclusive: string): WindowRate {
  const days = Math.max(0, daysBetween(start, endExclusive));
  const applications = days > 0 ? countApplications(job.overallApplications?.byDate, start, endExclusive) - countApplications(multiListingByDate(job) ?? undefined, start, endExclusive) : 0;
  return { days, applications, perDay: days > 0 ? applications / days : null };
}

/**
 * First day the content of periods[index] is known to be shown: earlier periods count only while
 * they touch and nothing changed between them (the same content was seen again).
 */
function runStart(periods: readonly TimelinePeriod[], ranges: readonly CountRange[], same: (index: number) => boolean, index: number): string {
  let cursor = index;
  while (cursor > 0 && same(cursor) && periods[cursor - 1]?.end === periods[cursor]?.start) cursor -= 1;
  return ranges[cursor]?.start ?? '';
}
/** Exclusive end of the days the content of periods[index] is known to be shown (same rule). */
function runEnd(periods: readonly TimelinePeriod[], ranges: readonly CountRange[], same: (index: number) => boolean, index: number, asOf: string): string {
  let cursor = index;
  while (cursor < periods.length - 1 && same(cursor + 1) && periods[cursor]?.end === periods[cursor + 1]?.start) cursor += 1;
  return ranges[cursor]?.end ?? addDays(asOf, 1);
}

export function overviewRow(job: JobCopyRecord, options: { billing?: readonly BillingEntry[] | undefined; now?: Date | undefined; windowDays?: number | undefined; dummyBilling?: boolean | undefined } = {}): OverviewRow {
  const windowDays = options.windowDays ?? OVERVIEW_WINDOW_DAYS;
  const asOf = asOfDate(job, options.now);
  const changes = versionChanges(job);
  const periods = buildPeriods(job, asOf);
  const ranges = countRanges(job, asOf, periods);
  const same = (index: number) => { const change = changes[index]; return change !== undefined && boundaryStatus(change) === 'same'; };
  const windowOf = (index: number): ChangeWindow | null => {
    const change = changes[index]; const period = periods[index]; const previous = periods[index - 1];
    if (!change || !period || !previous) return null;
    const exact = period.basis === 'published' && previous.end === period.start;
    return { from: exact ? period.start : previous.start, to: period.start, exact };
  };
  const changed = changes.map((change, index) => ({ change, index, window: windowOf(index) })).filter(item => item.index > 0 && changeKinds(item.change).length > 0 && item.window);
  const latest = changed.at(-1) ?? null;
  const applicationsAvailable = job.overallApplications?.byDate !== undefined;
  let before: WindowRate | null = null; let after: WindowRate | null = null;
  if (latest && applicationsAvailable) {
    // Before: up to the last day the earlier content is known to be shown (the day before 取得日A
    // when it changed after A), never earlier than the first such day. After: from the first day
    // the later content is known to be shown (the day after 取得日B), never past the last such day.
    // The acquisition days and the days in between are not counted on either side.
    const previousRange = ranges[latest.index - 1]; const range = ranges[latest.index];
    if (previousRange && range) {
      const beforeEnd = previousRange.end;
      const earliest = runStart(periods, ranges, same, latest.index - 1);
      const windowStart = addDays(beforeEnd, -windowDays);
      const beforeStart = windowStart < earliest ? earliest : windowStart;
      before = rate(job, beforeStart, beforeEnd < beforeStart ? beforeStart : beforeEnd);
      const known = runEnd(periods, ranges, same, latest.index, asOf);
      const limit = [addDays(range.start, windowDays), known, addDays(asOf, 1)].reduce((a, b) => (a < b ? a : b));
      after = rate(job, range.start, limit < range.start ? range.start : limit);
    }
  }
  const all = billingEntries(job, options.billing, { asOf, dummy: options.dummyBilling });
  const billing = realBilling(all);
  const dummy = dummyBilling(all);
  const known = billing.filter(entry => entry.amountYen !== null);
  const conflict = billingConflict(billing);
  const overlapping = billingOverlaps(billing);
  return {
    jobId: job.id, title: job.title, company: job.company, media: job.media,
    lastChange: latest?.window ?? null, kinds: latest ? changeKinds(latest.change) : [], changes: changed.flatMap(item => item.window ? [item.window] : []), firstDate: changes.find(change => change.date !== '')?.date ?? null,
    before, after, weeks: applicationBuckets(job.overallApplications?.byDate, 'week'),
    billingYen: known.length && !overlapping ? known.reduce((sum, entry) => sum + (entry.amountYen ?? 0), 0) : null,
    billingConnected: billing.length > 0, billingMissingAmount: billing.length > known.length, billingOverlapping: overlapping && !conflict, billingConflict: conflict, billingFictional: billing.some(entry => entry.fictional === true),
    hasDummyBilling: dummy.length > 0,
    reasonText: overviewReasonText(job.applicantReasons),
    applicationsAvailable, asOf,
  };
}

export function overviewRows(jobs: readonly JobCopyRecord[], options: { billing?: Readonly<Record<string, readonly BillingEntry[]>> | undefined; now?: Date | undefined; windowDays?: number | undefined; dummyBilling?: boolean | undefined } = {}): OverviewRow[] {
  return jobs.map(job => overviewRow(job, { billing: options.billing?.[job.id], now: options.now, windowDays: options.windowDays, dummyBilling: options.dummyBilling }));
}

export type OverviewSort = 'source' | 'lastChange' | 'afterPerDay' | 'beforePerDay' | 'billing';
/**
 * Missing values always go last; ties keep the original order. Billing sorts by the real total
 * only: a job with only the dummy billing has no value and goes last.
 */
export function sortOverview(rows: readonly OverviewRow[], sort: OverviewSort): OverviewRow[] {
  if (sort === 'source') return [...rows];
  const value = (row: OverviewRow): number | string | null => sort === 'lastChange' ? row.lastChange?.to ?? null
    : sort === 'afterPerDay' ? comparableRate(row.after) ? row.after?.perDay ?? null : null
      : sort === 'beforePerDay' ? comparableRate(row.before) ? row.before?.perDay ?? null : null : row.billingYen;
  return rows.map((row, index) => ({ row, index, value: value(row) }))
    .sort((a, b) => {
      if (a.value === null || b.value === null) return a.value === b.value ? a.index - b.index : a.value === null ? 1 : -1;
      if (a.value === b.value) return a.index - b.index;
      return a.value < b.value ? 1 : -1;
    })
    .map(item => item.row);
}

/** Shared calendar for the mini charts: earliest version / application day to the latest asOf. */
export function overviewRange(jobs: readonly JobCopyRecord[], rows: readonly OverviewRow[]): { start: string; end: string } | null {
  const days: string[] = [];
  // Version days come from the rows (versionChanges already ran once per job in overviewRow).
  for (const job of jobs) for (const date of Object.keys(job.overallApplications?.byDate ?? {})) days.push(date);
  for (const row of rows) {
    if (row.firstDate) days.push(row.firstDate);
    for (const change of row.changes) days.push(change.from, change.to);
    if (row.applicationsAvailable) days.push(row.asOf);
  }
  if (!days.length) return null;
  const sorted = days.filter(day => /^\d{4}-\d{2}-\d{2}$/.test(day)).sort();
  const start = sorted[0]; const end = sorted.at(-1);
  return start && end ? { start, end } : null;
}
