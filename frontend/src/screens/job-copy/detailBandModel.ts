/**
 * 案件の上の帯と右の「数字の前提」に出す数字。
 * 4週 = 28日。応募を確認した日（asOf）を最後の日として、直近4週とその前の4週を数える。
 * 増えた・減ったの判定語は付けず、件数と差だけを出す。
 */
import type { JobCopyRecord } from './data';
import { addDays, asOfDate, countApplications, daysBetween } from './timelineModel';
import type { BillingEntry } from './timelineModel';

const DATE = /^\d{4}-\d{2}-\d{2}$/;
const WINDOW_DAYS = 28;

export interface BandNumbers {
  /** 応募を確認した日（直近4週の最後の日）。 */
  asOf: string;
  /** 直近4週の応募。応募の日別データが無いときは null。 */
  recent: number | null;
  /** その前の4週の応募。 */
  previous: number | null;
  /** 課金 ÷ 応募（直近4週）。課金が不明、または応募が0件のとき null。 */
  perApplicationYen: number | null;
  /** 最新の課金期間の金額を4週あたりにした額。不明なとき null。 */
  billingPer4WeeksYen: number | null;
  /** 応募データのある最後の日。 */
  lastApplicationDate: string | null;
  /** 課金データの最後の日。 */
  lastBillingDate: string | null;
}

export function formatBandYen(value: number | null): string {
  return value === null ? '—' : `¥${Math.round(value).toLocaleString('ja-JP')}`;
}

/** 最新の課金期間を4週あたりに直す。同じ最終日で金額が違う行が並ぶときは決められないので null。 */
function latestBilling(entries: readonly BillingEntry[]): { per4Weeks: number | null; lastDate: string | null } {
  const valid = entries.filter(entry => DATE.test(entry.start) && DATE.test(entry.end) && entry.start <= entry.end);
  const lastDate = valid.reduce<string | null>((max, entry) => (max === null || entry.end > max ? entry.end : max), null);
  if (!lastDate) return { per4Weeks: null, lastDate: null };
  const latest = valid.filter(entry => entry.end === lastDate && entry.amountYen !== null);
  const amounts = new Set(latest.map(entry => {
    const days = daysBetween(entry.start, entry.end) + 1;
    return Math.round((entry.amountYen ?? 0) * WINDOW_DAYS / days);
  }));
  const only = amounts.size === 1 ? [...amounts][0] : undefined;
  return { per4Weeks: only ?? null, lastDate };
}

export function bandNumbers(job: JobCopyRecord, billing: readonly BillingEntry[] | undefined): BandNumbers {
  const asOf = asOfDate(job);
  const byDate = job.overallApplications?.byDate;
  const recentStart = addDays(asOf, -(WINDOW_DAYS - 1));
  const endExclusive = addDays(asOf, 1);
  const previousStart = addDays(recentStart, -WINDOW_DAYS);
  const recent = byDate ? countApplications(byDate, recentStart, endExclusive) : null;
  const previous = byDate ? countApplications(byDate, previousStart, recentStart) : null;
  const dates = Object.keys(byDate ?? {}).filter(date => DATE.test(date)).sort();
  const { per4Weeks, lastDate } = latestBilling(billing ?? []);
  const perApplicationYen = per4Weeks !== null && recent ? Math.round(per4Weeks / recent) : null;
  return { asOf, recent, previous, perApplicationYen, billingPer4WeeksYen: per4Weeks, lastApplicationDate: dates.at(-1) ?? null, lastBillingDate: lastDate };
}
