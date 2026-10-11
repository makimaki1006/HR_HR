import { formatDateTimeJst } from './format';
import { hrhCopySections, composeHrhBody } from './hrhCopy';
import type { JobCopyRecord } from './data';
export interface HubSpotListing {
  id: string; media: 'hrh' | 'airwork'; media_job_id: string; account_id: string | null;
  title: string | null; prefecture: string | null; municipality: string | null; category: string | null;
  publication_status: string | null; last_csv_detected_at: string | null; application_count: number | null;
}
interface ListingPageBase { listings: HubSpotListing[]; titles: string[]; offset: number; next_offset: number | null; refreshing: boolean; refresh_failed: boolean }
export type HubSpotListingPage = ListingPageBase & (
  { status: 'ready'; total: number; index_built_at: string } |
  { status: 'preparing'; total: null; index_built_at: null }
);
export interface HubSpotVersions {
  company_name?: string | null;
  current?: { checked_at: string; body: string; image_urls: string[] | null } | null;
  current_images?: { observed_at: string; image_urls: string[] } | null;
  listing: HubSpotListing;
  versions: { written_at: string; body: string; image_urls: string[] | null }[];
  history_counts: Record<string, number>; history_may_be_incomplete: boolean;
}
export const mediaLabel = (media: HubSpotListing['media']) => media === 'hrh' ? 'HRハッカー' : 'AirWork';
export function listingRecord(data: HubSpotVersions): JobCopyRecord {
  const row = data.listing;
  const copy = (body: string) => row.media === 'hrh' ? { body: composeHrhBody(body), bodySections: hrhCopySections(body) } : { body, bodySections: body.trim() ? [{ heading: '仕事内容', text: body }] : [] };
  return {
    id: `hubspot-history-${row.id}`, hubspotId: row.id, historyMayBeIncomplete: data.history_may_be_incomplete, dataSource: 'hubspot',
    title: row.title ?? '求人名未取得', company: data.company_name ?? '', media: mediaLabel(row.media),
    mediaJobId: row.media_job_id, ...(row.account_id ? { accountId: row.account_id } : {}),
    location: [row.prefecture, row.municipality].filter(Boolean).join('') || '',
    ...(data.current_images ? { currentImageObservation: { observedAt: data.current_images.observed_at, images: data.current_images.image_urls.map((url, index) => ({ id: `observed-${String(index)}`, url, caption: `現在の画像${String(index + 1)}` })) } } : {}),
    versions: [...data.versions.map((version, index) => ({
      id: `hubspot-history-${row.id}-${String(index)}`, label: `${formatDateTimeJst(version.written_at, '日時不明')} にHubSpotに記録された本文`,
      observedAt: version.written_at, ...copy(version.body), kind: 'published' as const, certainty: 'unknown' as const,
      source: 'HubSpotに記録された本文', applications: null,
      ...(version.image_urls !== null ? { images: version.image_urls.map((url, slot) => ({ id: `history-image-${String(slot)}`, url, caption: `画像${String(slot + 1)}` })) } : {}),
      historicalImageBytesAvailable: false,
      note: '本文がHubSpotに記録された日時です。掲載開始日時は不明です。画像は当時と異なる可能性があります。この本文への応募件数は未取得です。',
    })), ...(data.current ? [{ id: `hubspot-current-${row.id}`, label: `${formatDateTimeJst(data.current.checked_at, '日時不明')} に確認した現在の本文`, observedAt: data.current.checked_at, ...copy(data.current.body), kind: 'published' as const, certainty: 'unknown' as const, source: 'HubSpotの現在の本文', applications: null, ...(data.current.image_urls !== null ? { images: data.current.image_urls.map((url, slot) => ({ id: `current-${String(slot)}`, url, caption: `画像${String(slot + 1)}` })) } : {}), historicalImageBytesAvailable: false, note: '以前の本文を確認できないため、現在の本文を表示しています。日時は今回本文を確認した日時です。本文が記録された日時と掲載開始日時は不明です。' }] : [])],
  };
}
