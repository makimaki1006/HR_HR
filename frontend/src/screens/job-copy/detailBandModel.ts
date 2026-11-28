/**
 * 求人のページの上の帯（数字とデータの最後の日）に出す数字。
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
  /** 直近4週の始まりの日（終わりは asOf）。asOf が無いとき null。 */
  recentStart: string | null;
  /** 課金 ÷ 応募（直近4週）。課金が不明、または応募が0件のとき null。 */
  perApplicationYen: number | null;
  /** 直近4週（応募と同じ28日）の課金を日割りで足した額。窓に課金の記録が無い・金額が不明なとき null。 */
  billingRecentYen: number | null;
  /** 応募データのある最後の日。 */
  lastApplicationDate: string | null;
  /** 課金データの最後の日。 */
  lastBillingDate: string | null;
}

export function formatBandYen(value: number | null): string {
  return value === null ? '—' : `¥${Math.round(value).toLocaleString('ja-JP')}`;
}

/** 課金データの最後の日。 */
function lastBillingDay(entries: readonly BillingEntry[]): string | null {
  return entries.filter(entry => DATE.test(entry.start) && DATE.test(entry.end) && entry.start <= entry.end)
    .reduce<string | null>((max, entry) => (max === null || entry.end > max ? entry.end : max), null);
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
  const base = { asOf, lastApplicationDate: dates.at(-1) ?? null, lastBillingDate: lastBillingDay(billing ?? []) };
  if (!asOf) return { ...base, recentStart: null, recent: null, previous: null, perApplicationYen: null, billingRecentYen: null };
  const recentStart = addDays(asOf, -(WINDOW_DAYS - 1));
  const previousStart = addDays(recentStart, -WINDOW_DAYS);
  const recent = byDate ? countApplications(byDate, recentStart, addDays(asOf, 1)) : null;
  const previous = byDate ? countApplications(byDate, previousStart, recentStart) : null;
  const windowBilling = billingInWindow(billing ?? [], recentStart, asOf);
  const perApplicationYen = windowBilling !== null && recent ? Math.round(windowBilling / recent) : null;
  // 3 枚のカード（応募・応募1件の費用・課金）は同じ28日で数え、割り算が表示どうしで合うようにする。
  return { ...base, recentStart, recent, previous, perApplicationYen, billingRecentYen: windowBilling };
}

/**
 * 応募の日別データが無いときの、画面に出す文。「分かりません」は取得に失敗したときだけ。
 * 読み込み中・取得できなかった・この求人に応募の記録元が無い、を分ける。応募0件は「無い」ではなく0件として扱う（ここには来ない）。
 */
export function noApplicationDatesText(job: Pick<JobCopyRecord, 'overallApplications' | 'applicationDates'>): string {
  if (job.applicationDates === 'loading') return '応募を読み込み中です。';
  if (job.applicationDates === 'failed') return '応募を読み込めませんでした。';
  return 'この求人の応募はまだ記録されていません。';
}

/** 2026-06-10 → 6/10。今年でない日は 2025/6/10 のように年を付ける。 */
function shortDate(date: string, thisYear: string): string {
  const monthDay = `${String(Number(date.slice(5, 7)))}/${String(Number(date.slice(8, 10)))}`;
  return date.slice(0, 4) === thisYear ? monthDay : `${date.slice(0, 4)}/${monthDay}`;
}

/**
 * 上の帯に添える掲載の期間。「（6/10〜）」。ended（今の公開状況が掲載終了）のときだけ終了日を付けて「（6/10〜9/1）」、
 * 掲載終了なのに終了日が分からなければ「（6/10〜、終了日の記録なし）」。掲載中なら end があっても終了日は出さない。
 * 開始日が分からないときは空文字（何も出さない）。today は JST の今日（年を付けるかの判断だけに使う）。
 */
export function listingPeriodText(start: string | null, end: string | null, today: string, ended = false): string {
  if (!start || !DATE.test(start)) return '';
  const thisYear = today.slice(0, 4);
  if (!ended) return `（${shortDate(start, thisYear)}〜）`;
  return end && DATE.test(end) && end >= start ? `（${shortDate(start, thisYear)}〜${shortDate(end, thisYear)}）` : `（${shortDate(start, thisYear)}〜、終了日の記録なし）`;
}

export interface MarketMonthOverMonth {
  /** 市場の求人数がある最後の月（YYYY-MM）。 */
  month: string;
  jobs: number;
  /** その前の月。 */
  previousMonth: string;
  previousJobs: number | null;
  /** 前月比（%）。前月の数字が無い・前月が0件のとき null。 */
  changePct: number | null;
}

/** 競合の求人数の最新の月と、その前の月との比較。求人数が1か月も無いとき null。 */
export function marketMonthOverMonth(rows: readonly { month: string; jobs: number | null }[]): MarketMonthOverMonth | null {
  const latest = rows.filter((row): row is { month: string; jobs: number } => row.jobs !== null).sort((a, b) => a.month.localeCompare(b.month)).at(-1);
  if (!latest) return null;
  const total = Number(latest.month.slice(0, 4)) * 12 + Number(latest.month.slice(5, 7)) - 2;
  const previousMonth = `${String(Math.floor(total / 12))}-${String(total % 12 + 1).padStart(2, '0')}`;
  const previousJobs = rows.find(row => row.month === previousMonth)?.jobs ?? null;
  const changePct = previousJobs !== null && previousJobs > 0 ? (latest.jobs - previousJobs) / previousJobs * 100 : null;
  return { month: latest.month, jobs: latest.jobs, previousMonth, previousJobs, changePct };
}

/** 割合の差の文字。判定の言葉は付けず、符号と割合だけ（+3%・−12%・±0%）。 */
export function signedPct(pct: number): string {
  const rounded = Math.round(pct);
  return `${rounded > 0 ? '+' : rounded < 0 ? '−' : '±'}${String(Math.abs(rounded))}%`;
}
