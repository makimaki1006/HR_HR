/**
 * 変更ごとの応募理由: 給与・本文・画像を変えた日の前後 N 週に、応募の件数と応募理由の記録を並べる。
 * 割合や「効いた」という判定は作らない。区間の取り方は一覧表(overviewModel)と同じで、
 * 文面を確認した日と、その間の日はどちらの側にも数えない。
 */
import type { JobCopyRecord } from './data';
import type { ApplicantReason } from './applicantReasonsModel';
import { reasonSourceLabels } from './applicantReasonsModel';
import { buildListingEvents, md } from './listingEventsModel';
import { changeWindowOf, rate, runEndIndex, runStartIndex } from './overviewModel';
import type { ChangeWindow } from './overviewModel';
import { classifyApplicationReasons, tally } from './reasonCategories';
import type { ReasonTally } from './reasonCategories';
import { addDays, boundaryStatus, buildPeriods, changeKinds, countRanges, daysBetween, versionChanges } from './timelineModel';

export type WindowLimit = 'previous_change' | 'next_change' | 'listing_start' | 'today';
export type ReasonWeeks = 2 | 4 | 8;

export interface ReasonText { id: string; text: string; applicationDate: string | null; sourceLabel: string }
export interface ReasonWindow {
  /** [start, end): 最後の日は end の前日。 */
  start: string;
  end: string;
  days: number;
  /** 応募の件数。応募の日別件数を取得していないときは null（0 ではない）。 */
  applications: number | null;
  /** 応募理由の記録。収集できていないときは null。 */
  reasons: ReasonTally | null;
  texts: ReasonText[];
  /** 週数の長さに満たないとき、その理由と区切りになった変更・開始の日。 */
  shortened: { reason: WindowLimit; date: string | null } | null;
}
export interface ChangeReasonBlock {
  versionId: string;
  window: ChangeWindow;
  kinds: ('給与' | '本文' | '画像')[];
  title: string;
  summary: string;
  before: ReasonWindow;
  after: ReasonWindow;
}
export interface ChangeReasonSummary {
  /** 応募理由を収集できているか。 */
  reasonsAvailable: boolean;
  /** 応募日が分からず、どの期間にも入れていない記録（応募ごと）。 */
  undated: number;
  /** ほかの求人にも関連するため数えていない応募。分からないときは null。 */
  multiListing: number | null;
}

export function changeReasonSummary(job: JobCopyRecord): ChangeReasonSummary {
  const classified = classifyApplicationReasons(job.applicantReasons);
  return {
    reasonsAvailable: classified !== null,
    undated: classified ? classified.applications.filter(application => application.applicationDate === null).length : 0,
    multiListing: classified?.multiListing ?? null,
  };
}

/** 日付の表記: 日が決まるなら「9/25」、取得日の間なら「9/10〜9/28 の間」。 */
export function changeDateLabel(window: ChangeWindow): string {
  return window.exact ? md(window.to) : `${md(window.from)}〜${md(window.to)} の間`;
}

export function changeReasonBlocks(job: JobCopyRecord, weeks: ReasonWeeks, asOf: string): ChangeReasonBlock[] {
  const changes = versionChanges(job);
  const periods = buildPeriods(job, asOf);
  const ranges = countRanges(job, asOf, periods);
  const same = (index: number) => { const change = changes[index]; return change !== undefined && boundaryStatus(change) === 'same'; };
  const events = buildListingEvents(job, []);
  const classified = classifyApplicationReasons(job.applicantReasons);
  const applicationsAvailable = job.overallApplications?.byDate !== undefined;
  const length = weeks * 7;
  const tomorrow = addDays(asOf, 1);

  const windowFor = (start: string, end: string, shortened: ReasonWindow['shortened']): ReasonWindow => {
    const from = start;
    const to = end < start ? start : end;
    const inside = classified ? classified.applications.filter(application => application.applicationDate !== null && application.applicationDate >= from && application.applicationDate < to) : null;
    const texts: ReasonText[] = inside ? inside.flatMap(application => application.texts.map((item: ApplicantReason) => ({
      id: item.id, text: item.text, applicationDate: item.applicationDate ?? application.applicationDate, sourceLabel: reasonSourceLabels[item.sourceProperty] ?? 'HubSpotの記録欄',
    }))).sort((a, b) => (a.applicationDate ?? '').localeCompare(b.applicationDate ?? '')) : [];
    return {
      start: from, end: to, days: Math.max(0, daysBetween(from, to)),
      applications: applicationsAvailable ? rate(job, from, to).applications : null,
      reasons: inside ? tally(inside) : null, texts, shortened,
    };
  };

  const blocks: ChangeReasonBlock[] = [];
  changes.forEach((change, index) => {
    const kinds = changeKinds(change);
    const window = changeWindowOf(periods, index);
    const previousRange = ranges[index - 1]; const range = ranges[index];
    if (index === 0 || kinds.length === 0 || !window || !previousRange || !range) return;
    // 前: 前の内容が出ていたと分かる最後の日まで、その内容が続いた最初の日より前には戻らない。
    const beforeEnd = previousRange.end;
    const startIndex = runStartIndex(periods, same, index - 1);
    const earliest = ranges[startIndex]?.start ?? beforeEnd;
    const wanted = addDays(beforeEnd, -length);
    const beforeShort = wanted < earliest;
    const beforeStart = beforeShort ? earliest : wanted;
    const beforeLimit: ReasonWindow['shortened'] = !beforeShort ? null : startIndex === 0 ? { reason: 'listing_start', date: changes[0]?.date ?? null } : { reason: 'previous_change', date: changes[startIndex]?.date ?? null };
    // 後: 後の内容が出ていたと分かる最初の日から、同じ内容が続く最後の日・応募を数えた日まで。
    const endIndex = runEndIndex(periods, same, index);
    const known = ranges[endIndex]?.end ?? tomorrow;
    const fullEnd = addDays(range.start, length);
    const afterEnd = [fullEnd, known, tomorrow].reduce((a, b) => (a < b ? a : b));
    let afterLimit: ReasonWindow['shortened'] = null;
    if (afterEnd < fullEnd) afterLimit = afterEnd === known && endIndex < periods.length - 1 ? { reason: 'next_change', date: changes[endIndex + 1]?.date ?? null } : { reason: 'today', date: addDays(afterEnd, -1) };
    const event = events.find(item => item.versionId === change.versionId && (item.kind === 'content' || item.kind === 'image'));
    blocks.push({
      versionId: change.versionId, window, kinds,
      title: event?.title ?? `${kinds.join('と')}を変更`, summary: event?.summary ?? '',
      before: windowFor(beforeStart, beforeEnd, beforeLimit),
      after: windowFor(range.start, afterEnd, afterLimit),
    });
  });
  return blocks.reverse();
}
