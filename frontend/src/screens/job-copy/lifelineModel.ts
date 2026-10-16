/**
 * 求人のライフライン: 掲載開始から掲載終了まで（掲載中なら今日まで）を、1週1列で並べる。
 * 週は月曜はじまり。行は 課金・閲覧数・クリック数・応募数（HubSpot に記録された応募日）・応募数（媒体の集計）。
 * 0 は 0 として出す。データの元が無い行だけ null（画面では「—」か「データなし」）。
 */
import type { JobCopyRecord } from './data';
import { billingInWindow } from './detailBandModel';
import type { ListingEvent } from './listingEventsModel';
import { addDays, countApplications, daysBetween, todayJst } from './timelineModel';
import type { BillingEntry } from './timelineModel';

export const COLUMN_PX = 56;

export interface LifelineWeek {
  /** 月曜の日付。 */
  start: string;
  /** この週の本文・給与・画像の変更（連携開始は除く）。 */
  changes: ListingEvent[];
  billing: number | null;
  impressions: number | null;
  clicks: number | null;
  /** HubSpot に記録された応募（応募日がこの週のもの）。応募の日別データが無いとき null。 */
  hubspotApplications: number | null;
  /** 媒体（Indeed / AirWork）の集計の応募数。 */
  mediaApplications: number | null;
}
export interface Lifeline {
  weeks: LifelineWeek[];
  /** 掲載開始・掲載終了・今日の列（無ければ null）。 */
  startIndex: number | null;
  endIndex: number | null;
  todayIndex: number | null;
  /** 週ごとの実績（閲覧数・クリック数・媒体の応募数）の元データがあるか。 */
  hasPerformance: boolean;
  mediaLabel: 'Indeed' | 'AirWork';
}

export const mondayOf = (date: string) => addDays(date, -((new Date(`${date}T00:00:00Z`).getUTCDay() + 6) % 7));

/** 日付が入る列の番号。範囲の外は null。 */
export function weekIndex(lifeline: Pick<Lifeline, 'weeks'>, date: string): number | null {
  const first = lifeline.weeks[0]?.start;
  if (!first) return null;
  const index = Math.floor(daysBetween(first, date) / 7);
  return index >= 0 && index < lifeline.weeks.length ? index : null;
}

export function buildLifeline(job: JobCopyRecord, events: readonly ListingEvent[], billing: readonly BillingEntry[], now: Date = new Date()): Lifeline | null {
  const today = todayJst(now);
  const mediaLabel = job.media === 'AirWork' || job.media === 'Airワーク' ? 'AirWork' : 'Indeed';
  const startEvent = events.find(event => event.kind === 'start');
  const first = startEvent?.date ?? events.map(event => event.date).sort()[0];
  if (!first) return null;
  const lastState = [...events].reverse().find(event => event.kind === 'end' || event.kind === 'revive');
  const endDate = lastState?.kind === 'end' ? lastState.date : today;
  const lastDay = endDate < first ? first : endDate;
  const firstMonday = mondayOf(first);
  const weeks: LifelineWeek[] = [];
  const perf = job.hrhPerformance?.rows ?? [];
  const byDate = job.overallApplications?.byDate;
  for (let monday = firstMonday; monday <= lastDay; monday = addDays(monday, 7)) {
    const sunday = addDays(monday, 6);
    const row = perf.find(item => item.period_start <= monday && monday <= item.period_end);
    weeks.push({
      start: monday,
      changes: events.filter(event => (event.kind === 'content' || event.kind === 'image') && event.previousVersionId !== null && event.date >= monday && event.date <= sunday),
      billing: billingInWindow(billing, monday, sunday),
      impressions: row ? row.impressions : null,
      clicks: row ? row.clicks : null,
      hubspotApplications: byDate ? countApplications(byDate, monday, addDays(sunday, 1)) : null,
      mediaApplications: row ? row.applications : null,
    });
  }
  const lifeline: Lifeline = { weeks, startIndex: null, endIndex: null, todayIndex: null, hasPerformance: perf.length > 0, mediaLabel };
  lifeline.startIndex = startEvent ? weekIndex(lifeline, startEvent.date) : null;
  lifeline.endIndex = lastState?.kind === 'end' ? weekIndex(lifeline, lastState.date) : null;
  lifeline.todayIndex = lastState?.kind === 'end' ? null : weekIndex(lifeline, today);
  return lifeline;
}

/** 金額などの詳細を省き、印に収まる変更名にする。 */
export function changeMarkerLabel(events: readonly ListingEvent[]): string {
  if (events.length > 1) return `変更×${String(events.length)}`;
  const event = events[0];
  if (!event) return '';
  const title = event.title.replace(/（.*$/u, '');
  const kinds = ['本文', '給与', '画像'].filter(kind => title.includes(kind));
  return `${kinds.length ? kinds.join('・') : event.kind === 'image' ? '画像' : '本文'}変更`;
}
