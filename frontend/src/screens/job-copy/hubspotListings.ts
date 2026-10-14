import { formatDateTimeJst } from './format';
import { parseHrhPerformance } from './hrhPerformanceModel';
import type { HrhPerformanceCollection } from './hrhPerformanceModel';
import type { DraftSnapshot } from '../../generated/DraftSnapshot';
import type { CopyVersion } from './data';
import columns from '../../generated/jobgen/columns.json';
import { hrhCopySections, composeHrhBody } from './hrhCopy';
import type { JobCopyRecord } from './data';
import type { IndeedTitleState } from './marketMatch';
export interface HubSpotListing {
  id: string; media: 'hrh' | 'airwork'; media_job_id: string; account_id: string | null;
  title: string | null; prefecture: string | null; municipality: string | null; category: string | null;
  /** 人が指定した「比べる Indeed 職種」。state: set / none（市場と比べない）/ unset。古い応答には無い。 */
  indeed_title?: string | null; indeed_title_state?: IndeedTitleState;
  draft_status?: DraftSnapshot['review_status'] | null;
  deal_ids?: string[]; summary?: ListingSummary;
  publication_status: string | null; last_csv_detected_at: string | null; application_count: number | null;
}
export interface ListingSummary { occupation: string | null; location: string | null; salary_kind: string | null; salary_min: string | null; salary_max: string | null; hours: string | null; holidays: string | null; body_changed_at: string | null }
export interface DealCandidate { id: string; name: string | null; count: number }
export interface DealSelection { id: string; name: string; prefecture: string; title: string; media: string; publication?: string; draftStatus?: string }
export const publicationLabel = (value: string | null) => {
  switch (value?.trim()) {
    case '公開': case '公開中': case '掲載中': return '掲載中';
    case '非公開': case '公開終了': case '掲載終了': return '掲載終了';
    case '公開開始前': case '公開前': case '掲載前': return '掲載前';
    default: return '不明';
  }
};
interface ListingPageBase { publication_statuses?: string[]; deals?: DealCandidate[]; no_deal_count?: number; listings: HubSpotListing[]; titles: string[]; offset: number; next_offset: number | null; refreshing: boolean; refresh_failed: boolean }
export type HubSpotListingPage = ListingPageBase & (
  { status: 'ready'; total: number; index_built_at: string } |
  { status: 'preparing'; total: null; index_built_at: null }
);
export interface HubSpotVersions {
  current_fields?: Record<string, string>;
  current_row?: Record<string, string> | null;
  company_name?: string | null;
  drafts?: DraftSnapshot[];
  draft_revision?: string;
  can_write_drafts?: boolean;
  draft_history_may_be_incomplete?: boolean;
  current?: { checked_at: string; body: string; image_urls: string[] | null; images_registered?: boolean | null } | null;
  current_images_registered?: boolean | null;
  current_images?: { observed_at: string; image_urls: string[] } | null;
  /** HRハッカー求人の Indeed 週ごとの実績（HubSpot の indeed_weekly_perf）。無い・読めないときは null。 */
  indeed_weekly_perf?: unknown;
  listing: HubSpotListing;
  versions: { written_at: string; body: string; image_urls: string[] | null; images_registered?: boolean | null }[];
  history_counts: Record<string, number>; history_may_be_incomplete: boolean;
}
export const draftStateLabel = (status: DraftSnapshot['review_status']) => ({ pending: '確認待ち', adopted: '採用', rejected: '見送り' })[status];
export function draftVersion(draft: DraftSnapshot, index: number): CopyVersion {
  const raw = columns.map(column => `${column}：${draft.row[column] ?? ''}`).join('\n');
  return { id: `hubspot-draft-${draft.draft_id}`, draft, label: `${draftStateLabel(draft.review_status)}の案 ${String(index + 1)}`, observedAt: draft.created_at, comparisonBody: raw, body: composeHrhBody(raw), bodySections: hrhCopySections(raw), kind: 'ai_draft', certainty: 'unknown', applications: null, source: draft.manual_edit ? '手直しで保存した案' : '求人票作成で保存した案', note: '未掲載の案です。掲載開始日時と応募は対応させていません。' };
}
export const mediaLabel = (media: HubSpotListing['media']) => media === 'hrh' ? 'HRハッカー' : 'AirWork';
/** HubSpot に保存された Indeed の週ごとの実績。HRハッカー求人で、求人番号が一致し、形が正しいときだけ使う。 */
export function indeedWeeklyPerformance(raw: unknown, media: HubSpotListing['media'], mediaJobId: string): HrhPerformanceCollection | undefined {
  if (media !== 'hrh' || raw === null || raw === undefined) return undefined;
  try { return parseHrhPerformance(raw, mediaJobId); } catch { return undefined; }
}
export function listingRecord(data: HubSpotVersions, listedRow?: HubSpotListing): JobCopyRecord {
  const row = data.listing;
  const copy = (body: string) => row.media === 'hrh' ? { comparisonBody: body, body: composeHrhBody(body), bodySections: hrhCopySections(body) } : { body, bodySections: body.trim() ? [{ heading: '仕事内容', text: body }] : [] };
  return {
    currentImagesRegistered: data.current_images_registered ?? undefined,
    ...(indeedWeeklyPerformance(data.indeed_weekly_perf, row.media, row.media_job_id) ? { hrhPerformance: indeedWeeklyPerformance(data.indeed_weekly_perf, row.media, row.media_job_id) } : {}),
    id: `hubspot-history-${row.id}`, hubspotId: row.id, historyMayBeIncomplete: data.history_may_be_incomplete, dataSource: 'hubspot',
    currentDraftRow: data.current_row ?? undefined, draftRevision: data.draft_revision, canWriteDrafts: data.can_write_drafts, draftHistoryMayBeIncomplete: data.draft_history_may_be_incomplete, latestDraftId: data.drafts?.at(-1)?.draft_id,
    ...(data.current_fields ? { currentListingFields: data.current_fields } : {}),
    listingPublicationStatus: row.publication_status, listingApplicationCount: row.application_count ?? (listedRow?.id === row.id ? listedRow.application_count : null),
    title: row.title ?? '求人名未取得', company: data.company_name ?? '', media: mediaLabel(row.media),
    mediaJobId: row.media_job_id, ...(row.account_id ? { accountId: row.account_id } : {}),
    location: [row.prefecture, row.municipality].filter(Boolean).join('') || '',
    indeedTitle: row.indeed_title ?? null, indeedTitleState: row.indeed_title_state ?? 'unset',
    ...(data.current_images ? { currentImageObservation: { observedAt: data.current_images.observed_at, images: data.current_images.image_urls.map((url, index) => ({ id: `observed-${String(index)}`, url, caption: `現在の画像${String(index + 1)}` })) } } : {}),
    versions: [...data.versions.map((version, index) => ({
      id: `hubspot-history-${row.id}-${String(index)}`, label: `${formatDateTimeJst(version.written_at, '日時不明')} にHubSpotに記録された本文`,
      observedAt: version.written_at, ...copy(version.body), kind: 'published' as const, certainty: 'unknown' as const,
      source: 'HubSpotに記録された本文', applications: null,
      ...(version.image_urls !== null ? { images: version.image_urls.map((url, slot) => ({ id: `history-image-${String(slot)}`, url, caption: `画像${String(slot + 1)}` })) } : {}),
      imagesRegistered: version.images_registered ?? undefined,
      historicalImageBytesAvailable: false,
      note: '本文がHubSpotに記録された日時です。掲載開始日時は不明です。画像は当時と異なる可能性があります。この本文への応募件数は未取得です。',
    })), ...(data.current ? [{ id: `hubspot-current-${row.id}`, label: `${formatDateTimeJst(data.current.checked_at, '日時不明')} に確認した現在の本文`, observedAt: data.current.checked_at, ...copy(data.current.body), imagesRegistered: data.current.images_registered ?? undefined, kind: 'published' as const, certainty: 'unknown' as const, source: 'HubSpotの現在の本文', applications: null, ...(data.current.image_urls !== null ? { images: data.current.image_urls.map((url, slot) => ({ id: `current-${String(slot)}`, url, caption: `画像${String(slot + 1)}` })) } : {}), historicalImageBytesAvailable: false, note: '以前の本文を確認できないため、現在の本文を表示しています。日時は今回本文を確認した日時です。本文が記録された日時と掲載開始日時は不明です。' }] : []), ...(data.drafts ?? []).map(draftVersion)],
  };
}
