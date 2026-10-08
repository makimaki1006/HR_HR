/**
 * 媒体の公開状況 (latest) of HRハッカー jobs, read from HubSpot (/api/job-copy/listing-status).
 *
 * Only the latest values exist: when publication started, the status the media CSV last showed,
 * the last day the job was in that CSV, and the end date set on the media. The set end date is a
 * plan (often years ahead, even for a job that has stopped), so it is never drawn as the end of
 * the bar. Past publication periods are not guessed. AirWork has no such dates.
 */
import type { JobCopyRecord } from './data';
import { addDays, formatDay, type TimelineRange } from './timelineModel';

export interface MediaPublication {
  /** First day of publication set on the media. */
  start: string | null;
  /** End day set on the media. A plan, not the day it really stopped. */
  plannedEnd: string | null;
  /** 'unknown': a status value this screen does not know. null: not acquired. */
  status: 'public' | 'private' | 'unknown' | null;
  /** Last day the job was in the media CSV. */
  lastInCsv: string | null;
  /** Day the change from 公開 to 非公開 was recorded (it stopped on that day or before). */
  privateRecordedOn: string | null;
}
/** 'unavailable': an HRハッカー job whose publication status could not be read. */
export type MediaPublicationState = MediaPublication | 'unavailable';

const calendarDay = (value: unknown): value is string => typeof value === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(value)
  && Number.isFinite(Date.parse(`${value}T00:00:00Z`)) && new Date(`${value}T00:00:00Z`).toISOString().slice(0, 10) === value;
const day = (value: unknown): string | null | undefined => value === null || value === undefined ? null : calendarDay(value) ? value : undefined;
const STATUSES = ['public', 'private', 'unknown'] as const;

/**
 * Listing ID -> publication (null for a listing that is not an HRハッカー job). A listing whose
 * values do not have the expected shape is left out (shown as 取得できませんでした, never guessed).
 * null when the reply itself has the wrong shape.
 */
export function parseListingStatus(data: unknown): Map<string, MediaPublication | null> | null {
  if (typeof data !== 'object' || data === null) return null;
  const listings = (data as { listings?: unknown }).listings;
  if (typeof listings !== 'object' || listings === null || Array.isArray(listings)) return null;
  const result = new Map<string, MediaPublication | null>();
  for (const [id, raw] of Object.entries(listings as Record<string, unknown>)) {
    if (!/^\d{1,30}$/.test(id) || typeof raw !== 'object' || raw === null) continue;
    const row = raw as Record<string, unknown>;
    if (row.hrhacker === false) { result.set(id, null); continue; }
    if (row.hrhacker !== true) continue;
    const start = day(row.start); const plannedEnd = day(row.planned_end); const lastInCsv = day(row.last_in_csv); const privateRecordedOn = day(row.private_recorded_on);
    const status = row.status === null || row.status === undefined ? null : STATUSES.find(value => value === row.status);
    if (start === undefined || plannedEnd === undefined || lastInCsv === undefined || privateRecordedOn === undefined || status === undefined) continue;
    result.set(id, { start, plannedEnd, status, lastInCsv, privateRecordedOn: status === 'private' ? privateRecordedOn : null });
  }
  return result;
}

/**
 * Puts the publication on every HRハッカー job read from HubSpot. listings null (the read failed):
 * those jobs get 'unavailable'. Other jobs are left as they are.
 */
export function withMediaPublication(records: readonly JobCopyRecord[], listings: Map<string, MediaPublication | null> | null): JobCopyRecord[] {
  return records.map(job => {
    if (job.dataSource !== 'hubspot' || !job.hubspotId || job.media !== 'HRハッカー') return job;
    const found = listings?.get(job.hubspotId);
    // null: HubSpot holds no HRハッカー job ID for it, so there is no publication to show.
    if (found === null) return job;
    return { ...job, mediaPublication: found ?? 'unavailable' };
  });
}

/**
 * withMediaPublication for the jobs of one snapshot only, kept by job id: edits made while the
 * status was read stay, and jobs from another source are not touched.
 */
export function withPublicationFor(current: readonly JobCopyRecord[], snapshot: readonly JobCopyRecord[], listings: Map<string, MediaPublication | null> | null): JobCopyRecord[] {
  const ids = new Set(snapshot.map(job => job.id));
  const updated = new Map(withMediaPublication(current.filter(job => ids.has(job.id)), listings).map(job => [job.id, job]));
  return current.map(job => updated.get(job.id) ?? job);
}

export interface PublicationLane {
  /** The bar: days known to be published, clipped to the range. null when no day is known. */
  bar: { start: string; endExclusive: string; status: 'public' | 'private' } | null;
  /** The publication start is before the range (the bar starts at the left edge). */
  startsBefore: string | null;
  /** Short text on the bar or in the lane. */
  label: string;
  /** The tooltip: every value with what it means. */
  title: string;
}

export function publicationLane(publication: MediaPublicationState, range: TimelineRange): PublicationLane {
  if (publication === 'unavailable') return { bar: null, startsBefore: null, label: '媒体の公開状況は取得できませんでした（時間をおいて開き直してください）', title: '媒体の公開状況は取得できませんでした。' };
  const { start, status, lastInCsv, privateRecordedOn } = publication;
  const clipped = start !== null && start < range.start;
  const startText = start ? `公開開始 ${formatDay(start)}${clipped ? '（表示期間より前）' : ''}` : '公開開始日は未取得';
  // The status first: a short bar cuts the end of the text.
  const statusText = status === 'public' ? lastInCsv ? `${formatDay(lastInCsv)}時点で公開` : '公開（確認した日は未取得）'
    : status === 'private' ? privateRecordedOn ? `非公開（${formatDay(privateRecordedOn)}までに終了）` : '非公開（終了した日は不明）'
      : status === 'unknown' ? '公開状況は不明' : '公開状況は未取得';
  const label = `${statusText}・${startText}`;
  const title = publicationNotes(publication).join('\n');
  // From the start to the last day it was seen published (公開), or up to the day the stop was
  // recorded (非公開; that day is left out). Whether it was published on every day between is not
  // known, so the bar is drawn hatched.
  const end = status === 'public' && lastInCsv ? addDays(lastInCsv, 1) : status === 'private' && privateRecordedOn ? privateRecordedOn : null;
  if (!start || !end || end <= start || status === null || status === 'unknown') return { bar: null, startsBefore: null, label, title };
  if (end <= range.start || start > range.end) return { bar: null, startsBefore: null, label: `${label}（表示期間の外）`, title };
  return { bar: { start: clipped ? range.start : start, endExclusive: end > addDays(range.end, 1) ? addDays(range.end, 1) : end, status }, startsBefore: clipped ? start : null, label, title };
}

/** Sentences for the explanation (tooltip and the timeline's help): every value with what it means. */
export function publicationNotes(publication: MediaPublicationState): string[] {
  if (publication === 'unavailable') return ['媒体の公開状況は取得できませんでした。時間をおいて開き直してください。'];
  const { start, plannedEnd, lastInCsv } = publication;
  return [
    start ? `公開開始日: ${formatDay(start)}。` : '公開開始日は未取得です。',
    lastInCsv ? `HRハッカーの求人一覧に最後に載っていた日: ${formatDay(lastInCsv)}。` : '',
    plannedEnd ? `媒体に設定された公開終了日: ${formatDay(plannedEnd)}（予定の日付で、実際に終わった日ではありません）。` : '',
    '最新の状態だけを表示しています。帯は公開開始日から最後に公開を確認した日までで、その間ずっと公開していたか（途中で止めて再開したか）は分かりません。',
  ].filter(Boolean);
}
