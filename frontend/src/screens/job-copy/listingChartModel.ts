/**
 * 求人の画面の「週ごとの応募」グラフの配置。日付を軸の左端からの割合（%）にする。
 * 週は月曜はじまり。応募を取得した日（asOf）より先は「取得待ち」で、点線で区切る。
 */
import type { JobCopyRecord } from './data';
import { addDays, applicationBuckets, daysBetween, monthOf, todayJst } from './timelineModel';
import type { BillingEntry } from './timelineModel';
import { billingBands } from './listingEventsModel';
import type { ListingEvent } from './listingEventsModel';

const MAX_AFTER_CUTOFF_DAYS = 14;
/** 近い順にピンを段に分ける。軸の長さのこの割合より近いと重なるので次の段へ。 */
const PIN_GAP_PCT = 4.5;

export interface ChartBar { start: string; count: number; leftPct: number; widthPct: number }
export interface ChartPin { eventId: string; n: number; leftPct: number; row: number }
export interface ChartBand { label: string; leftPct: number; widthPct: number }
export interface ChartModel {
  start: string; end: string; totalDays: number;
  bars: ChartBar[]; maxCount: number; yTicks: number[];
  pins: ChartPin[]; bands: ChartBand[]; months: { label: string; leftPct: number }[];
  /** 応募を取得した日の位置（%）。 */
  cutoffPct: number;
}

const nextMonthStart = (date: string) => { const month = Number(date.slice(5, 7)); const year = Number(date.slice(0, 4)); return `${String(month === 12 ? year + 1 : year)}-${String(month === 12 ? 1 : month + 1).padStart(2, '0')}-01`; };
const mondayOf = (date: string) => addDays(date, -((new Date(`${date}T00:00:00Z`).getUTCDay() + 6) % 7));

/** 日付 → 軸上の位置（%）。軸の外は端に寄せる。 */
export function pct(date: string, start: string, totalDays: number): number {
  return Math.min(100, Math.max(0, daysBetween(start, date) / totalDays * 100));
}

export function buildChart(job: JobCopyRecord, events: readonly ListingEvent[], billing: readonly BillingEntry[], asOf: string, now: Date = new Date()): ChartModel | null {
  const byDate = job.overallApplications?.byDate;
  if (!byDate) return null;
  const buckets = applicationBuckets(byDate, 'week').filter(bucket => bucket.start <= asOf);
  const firstDates = [...events.map(event => event.date), ...buckets.map(bucket => bucket.start)].sort();
  const first = firstDates[0];
  if (!first) return null;
  const start = mondayOf(first);
  const gap = Math.max(0, Math.min(MAX_AFTER_CUTOFF_DAYS, daysBetween(asOf, todayJst(now))));
  const end = addDays(asOf, gap);
  const totalDays = Math.max(7, daysBetween(start, end) + 1);
  // 取得日までの毎週を、応募が0件の週も含めて並べる。
  const counts = new Map(buckets.map(bucket => [bucket.start, bucket.count]));
  const bars: ChartBar[] = [];
  for (let week = start; week <= asOf; week = addDays(week, 7)) {
    bars.push({ start: week, count: counts.get(week) ?? 0, leftPct: pct(week, start, totalDays), widthPct: Math.min(7, daysBetween(week, addDays(asOf, 1))) / totalDays * 100 });
  }
  const maxValue = Math.max(1, ...bars.map(bar => bar.count));
  const step = maxValue <= 8 ? 2 : maxValue <= 20 ? 5 : maxValue <= 50 ? 10 : 20;
  const top = Math.ceil(maxValue / step) * step;
  const yTicks: number[] = [];
  for (let value = 0; value <= top; value += step) yTicks.push(value);
  const lastPinPct: number[] = [];
  const pins: ChartPin[] = events.map(event => {
    const left = pct(event.date, start, totalDays);
    let row = lastPinPct.findIndex(last => Math.abs(left - last) >= PIN_GAP_PCT);
    if (row < 0) row = lastPinPct.length;
    lastPinPct[row] = left;
    return { eventId: event.id, n: event.n, leftPct: left, row };
  });
  const bands: ChartBand[] = billingBands(billing)
    .filter(band => band.end >= start && band.start <= end)
    .map(band => {
      const from = pct(band.start < start ? start : band.start, start, totalDays);
      const to = pct(addDays(band.end > end ? end : band.end, 1), start, totalDays);
      return { label: `¥${band.per4.toLocaleString('ja-JP')}`, leftPct: from, widthPct: Math.max(0, to - from) };
    });
  const months: { label: string; leftPct: number }[] = [];
  for (let day = start; day <= end; day = addDays(day, 1)) {
    // 左端の月は、すぐ次の月の始まりが来るとき（10日以内）は出さない（ラベルが重なる）。
    const crowded = day === start && daysBetween(start, nextMonthStart(start)) <= 10;
    if ((day === start && !crowded) || day.endsWith('-01')) months.push({ label: `${String(Number(monthOf(day).slice(5, 7)))}月`, leftPct: pct(day, start, totalDays) });
  }
  return { start, end, totalDays, bars, maxCount: top, yTicks, pins, bands, months, cutoffPct: pct(addDays(asOf, 1), start, totalDays) };
}
