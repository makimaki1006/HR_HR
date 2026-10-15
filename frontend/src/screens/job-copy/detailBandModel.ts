/**
 * 案件の上の帯と右の「数字の前提」に出す数字。
 * 4週 = 28日。応募を確認した日（asOf）を最後の日として、直近4週とその前の4週を数える。
 * 増えた・減ったの判定語は付けず、件数と差だけを出す。
 */
import type { JobCopyRecord } from './data';
import { addDays, countApplications, daysBetween, jstDate } from './timelineModel';
import type { BillingEntry } from './timelineModel';

const DATE = /^\d{4}-\d{2}-\d{2}$/;
const WINDOW_DAYS = 28;

export interface BandNumbers {
  /** 応募を確認した日（直近4週の最後の日）。 */
  asOf: string | null;
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

/**
 * 応募件数を取得した日（JST の日付）。「直近4週」の最後の日。
 * 取得日が分からないときは今日で代用せず null を返す。案件の画面でも同じ基準を使う。
 */
export function applicationsCutoffDate(job: { overallApplications?: { fetchedAt?: string | null | undefined } | undefined }): string | null {
  return jstDate(job.overallApplications?.fetchedAt);
}

/**
 * 窓（start〜end、両端を含む）に入る課金を日割りで足した額。
 * 窓に重なる期間に金額の分からないものがある、または期間どうしが重なるときは null。
 * 丸めは最後に1回だけ。
 */
export function billingInWindow(entries: readonly BillingEntry[], start: string, end: string): number | null {
  const inside = entries.filter(entry => DATE.test(entry.start) && DATE.test(entry.end) && entry.start <= entry.end && entry.start <= end && entry.end >= start);
  if (!inside.length) return null;
  if (inside.some(entry => entry.amountYen === null)) return null;
  const sorted = [...inside].sort((a, b) => a.start.localeCompare(b.start));
  if (sorted.some((entry, index) => index > 0 && entry.start <= (sorted[index - 1]?.end ?? ''))) return null;
  let total = 0;
  for (const entry of inside) {
    const days = daysBetween(entry.start, entry.end) + 1;
    const overlapStart = entry.start > start ? entry.start : start;
    const overlapEnd = entry.end < end ? entry.end : end;
    total += (entry.amountYen ?? 0) * (daysBetween(overlapStart, overlapEnd) + 1) / days;
  }
  return Math.round(total);
}

export function bandNumbers(job: JobCopyRecord, billing: readonly BillingEntry[] | undefined): BandNumbers {
  const asOf = applicationsCutoffDate(job);
  const byDate = job.overallApplications?.byDate;
  const dates = Object.keys(byDate ?? {}).filter(date => DATE.test(date)).sort();
  const { per4Weeks, lastDate } = latestBilling(billing ?? []);
  const base = { asOf, billingPer4WeeksYen: per4Weeks, lastApplicationDate: dates.at(-1) ?? null, lastBillingDate: lastDate };
  if (!asOf) return { ...base, recent: null, previous: null, perApplicationYen: null };
  const recentStart = addDays(asOf, -(WINDOW_DAYS - 1));
  const previousStart = addDays(recentStart, -WINDOW_DAYS);
  const recent = byDate ? countApplications(byDate, recentStart, addDays(asOf, 1)) : null;
  const previous = byDate ? countApplications(byDate, previousStart, recentStart) : null;
  const windowBilling = billingInWindow(billing ?? [], recentStart, asOf);
  const perApplicationYen = windowBilling !== null && recent ? Math.round(windowBilling / recent) : null;
  return { ...base, recent, previous, perApplicationYen };
}

/**
 * 応募の日別データが無いときの、画面に出す文。「分かりません」は取得に失敗したときだけ。
 * 読み込み中・取得できなかった・この求人に応募の記録元が無い、を分ける。応募0件は「無い」ではなく0件として扱う（ここには来ない）。
 */
export function noApplicationDatesText(job: Pick<JobCopyRecord, 'overallApplications' | 'applicationDates'>): string {
  if (job.applicationDates === 'loading') return '応募を読み込み中です。';
  if (job.applicationDates === 'failed') return '応募を取得できませんでした。時間を置いて、もう一度開いてください。';
  return 'この求人の応募はまだ記録されていません。';
}
