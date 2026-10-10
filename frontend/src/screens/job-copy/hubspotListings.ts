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
  listing: HubSpotListing;
  versions: { written_at: string; body: string; image_urls: string[] | null }[];
  history_counts: Record<string, number>; history_may_be_incomplete: boolean;
}
export const mediaLabel = (media: HubSpotListing['media']) => media === 'hrh' ? 'HRハッカー' : 'AirWork';
export function listingRecord(data: HubSpotVersions): JobCopyRecord {
  const row = data.listing;
  return {
    id: `hubspot-history-${row.id}`, hubspotId: row.id, dataSource: 'hubspot',
    title: row.title ?? '求人名未取得', company: '取引先名未取得', media: mediaLabel(row.media),
    mediaJobId: row.media_job_id, ...(row.account_id ? { accountId: row.account_id } : {}),
    location: [row.prefecture, row.municipality].filter(Boolean).join('') || '勤務地不明',
    versions: data.versions.map((version, index) => ({
      id: `hubspot-history-${row.id}-${String(index)}`, label: `取得した版 ${String(index + 1)}`,
      observedAt: version.written_at, body: version.body, kind: 'published', certainty: 'unknown',
      source: 'HubSpotに保存された求人の変更履歴', applications: null,
      ...(version.image_urls !== null ? { images: version.image_urls.map((url, slot) => ({ id: `history-image-${String(slot)}`, url, caption: `画像${String(slot + 1)}` })) } : {}),
      historicalImageBytesAvailable: false,
      note: '日時はHubSpotに保存された日時です。掲載開始日時は不明です。画像は記録されたURLで、保存当時の画像とは異なる可能性があります。応募は版に結びつけていません。',
    })),
  };
}
