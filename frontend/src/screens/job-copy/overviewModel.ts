/**
 * Cross-job overview: one row per job on a shared calendar. Shows the latest change and the
 * applications per day in the N days before and after it. Rows are sortable but never ranked as
 * winners; the numbers sit side by side and do not say why applications changed.
 */
import type { JobCopyRecord } from './data';
import {
  addDays, applicationBuckets, asOfDate, billingEntries, billingOverlaps, changeKinds, countApplications, daysBetween, dummyBilling, realBilling, versionChanges,
} from './timelineModel';
import type { ApplicationBucket, BillingEntry } from './timelineModel';

export const OVERVIEW_WINDOW_DAYS = 14;
/** A before/after window shorter than this is not compared (a rate from 1 or 2 days is not shown or sorted). */
export const MIN_RATE_DAYS = 7;
/** True when the window is long enough to put its rate beside others. */
export function comparableRate(rate: WindowRate | null): boolean {
  return rate !== null && rate.days >= MIN_RATE_DAYS && rate.perDay !== null;
}

export interface WindowRate {
  /**
   * Days actually covered. The before-window starts no earlier than the previous version; the
   * after-window stops at the day counts were taken.
   */
  days: number;
  applications: number;
  perDay: number | null;
}
export interface OverviewRow {
  jobId: string;
  title: string;
  company: string;
  media: string;
  /** Start day of the latest version that changed something; null with fewer than two versions. */
  lastChange: string | null;
  kinds: ('給与' | '本文' | '画像')[];
  /** Every version start after the first (change markers on the calendar). */
  changeDates: string[];
  /** Start day of the first version; null without a dated version. */
  firstDate: string | null;
  before: WindowRate | null;
  after: WindowRate | null;
  /** Applications recorded in HubSpot with a date, by week, for the mini calendar. */
  weeks: ApplicationBucket[];
  /** null: no billing data connected (not zero). */
  billingYen: number | null;
  billingConnected: boolean;
  billingMissingAmount: boolean;
  /** Two billing periods share days; the total is left blank instead of adding them up. */
  billingOverlapping: boolean;
  /** The real billing rows are the demo's made-up HRハッカー amounts. */
  billingFictional: boolean;
  /**
   * Total of the dummy billing (仮の課金データ) for the days with no real billing; null when none.
   * Shown separately and labelled, never added to billingYen.
   */
  dummyBillingYen: number | null;
  /** No application data at all (different from zero applications). */
  applicationsAvailable: boolean;
  asOf: string;
}

function rate(job: JobCopyRecord, start: string, endExclusive: string): WindowRate {
  const days = Math.max(0, daysBetween(start, endExclusive));
  const applications = countApplications(job.overallApplications?.byDate, start, endExclusive);
  return { days, applications, perDay: days > 0 ? applications / days : null };
}

export function overviewRow(job: JobCopyRecord, options: { billing?: readonly BillingEntry[] | undefined; now?: Date | undefined; windowDays?: number | undefined; dummyBilling?: boolean | undefined } = {}): OverviewRow {
  const windowDays = options.windowDays ?? OVERVIEW_WINDOW_DAYS;
  const asOf = asOfDate(job, options.now);
  const changes = versionChanges(job);
  const later = changes.slice(1);
  const latest = [...later].reverse().find(change => changeKinds(change).length > 0) ?? later.at(-1) ?? null;
  const applicationsAvailable = job.overallApplications?.byDate !== undefined;
  let before: WindowRate | null = null; let after: WindowRate | null = null;
  if (latest && applicationsAvailable) {
    // The before-window stays inside the previous version: it never reaches back into an earlier
    // version (or before the first publication day). Its real length is in before.days.
    const previousStart = changes[latest.index - 1]?.date ?? latest.date;
    const windowStart = addDays(latest.date, -windowDays);
    before = rate(job, windowStart < previousStart ? previousStart : windowStart, latest.date);
    const afterEnd = addDays(latest.date, windowDays);
    const cappedEnd = afterEnd > addDays(asOf, 1) ? addDays(asOf, 1) : afterEnd;
    after = rate(job, latest.date, cappedEnd);
  }
  const all = billingEntries(job, options.billing, { asOf, dummy: options.dummyBilling });
  const billing = realBilling(all);
  const dummy = dummyBilling(all);
  const known = billing.filter(entry => entry.amountYen !== null);
  return {
    jobId: job.id, title: job.title, company: job.company, media: job.media,
    lastChange: latest?.date ?? null, kinds: latest ? changeKinds(latest) : [], changeDates: later.map(change => change.date), firstDate: changes.find(change => change.date !== '')?.date ?? null,
    before, after, weeks: applicationBuckets(job.overallApplications?.byDate, 'week'),
    billingYen: known.length && !billingOverlaps(billing) ? known.reduce((sum, entry) => sum + (entry.amountYen ?? 0), 0) : null,
    billingConnected: billing.length > 0, billingMissingAmount: billing.length > known.length, billingOverlapping: billingOverlaps(billing), billingFictional: billing.some(entry => entry.fictional === true),
    dummyBillingYen: dummy.length ? dummy.reduce((sum, entry) => sum + (entry.amountYen ?? 0), 0) : null,
    applicationsAvailable, asOf,
  };
}

export function overviewRows(jobs: readonly JobCopyRecord[], options: { billing?: Readonly<Record<string, readonly BillingEntry[]>> | undefined; now?: Date | undefined; windowDays?: number | undefined; dummyBilling?: boolean | undefined } = {}): OverviewRow[] {
  return jobs.map(job => overviewRow(job, { billing: options.billing?.[job.id], now: options.now, windowDays: options.windowDays, dummyBilling: options.dummyBilling }));
}

export type OverviewSort = 'source' | 'lastChange' | 'afterPerDay' | 'beforePerDay' | 'billing';
/** Missing values always go last; ties keep the original order. */
export function sortOverview(rows: readonly OverviewRow[], sort: OverviewSort): OverviewRow[] {
  if (sort === 'source') return [...rows];
  const value = (row: OverviewRow): number | string | null => sort === 'lastChange' ? row.lastChange
    : sort === 'afterPerDay' ? comparableRate(row.after) ? row.after?.perDay ?? null : null
      : sort === 'beforePerDay' ? comparableRate(row.before) ? row.before?.perDay ?? null : null : row.billingYen;
  // Billing: rows with a real total come first (by the real total); rows with only the dummy total
  // follow, ordered among themselves. A dummy amount is never compared with a real one.
  const dummyGroup = (row: OverviewRow) => sort === 'billing' && row.billingYen === null && row.dummyBillingYen !== null;
  const sortValue = (row: OverviewRow) => dummyGroup(row) ? row.dummyBillingYen : value(row);
  return rows.map((row, index) => ({ row, index, value: sortValue(row), dummy: dummyGroup(row) }))
    .sort((a, b) => {
      if (a.dummy !== b.dummy && a.value !== null && b.value !== null) return a.dummy ? 1 : -1;
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
    for (const date of row.changeDates) if (date) days.push(date);
    if (row.applicationsAvailable) days.push(row.asOf);
  }
  if (!days.length) return null;
  const sorted = days.filter(day => /^\d{4}-\d{2}-\d{2}$/.test(day)).sort();
  const start = sorted[0]; const end = sorted.at(-1);
  return start && end ? { start, end } : null;
}
