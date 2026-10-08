export interface ApplicantReason {
  id: string;
  /**
   * Opaque key shared by every text and selection of one application (not the HubSpot ID). null in
   * a stored file written before 2026-10-08: texts are then counted one by one.
   */
  applicant: string | null;
  text: string;
  sourceProperty: string;
  applicationDate: string | null;
  collectedAt: string | null;
  versionId: string | null;
}
export interface ReasonSourceCounts { missing: number; blank: number; nonblank: number }
/** A category chosen in HubSpot (応募理由カテゴリ). value is the stored value, label the option name when known. */
export interface ReasonSelection {
  applicant: string;
  sourceProperty: string;
  value: string;
  label: string | null;
  applicationDate: string | null;
}
export interface ApplicantReasonCollection {
  available: boolean;
  basis: string;
  fetchedAt: string | null;
  totalApplicants: number;
  totalSourceValues: number;
  sourceCounts: Record<string, ReasonSourceCounts>;
  missing: number;
  blank: number;
  truncated: boolean;
  items: ApplicantReason[];
  /** null: the category sources were not read (a stored file written before they were added); not 0. */
  selections: ReasonSelection[] | null;
}

/** Every source, in the order the server reads them. Shown names are plain words. */
export const reasonSourceLabels: Record<string, string> = {
  oubodouki: '応募動機（HubSpot）',
  ouboriyuu_baitaikisai: '応募理由（媒体記載）',
  ouboriyuu_hiaringu: '応募理由（ヒアリング）',
  genshokumaeshokukaranotenshokuriyuu: '今の仕事・前の仕事から転職する理由',
  ouboriyuukategori_hiaringu: '応募理由の分類（ヒアリング）',
  ouboriyuukategori_baitaikisai: '応募理由の分類（媒体記載）',
};
/** Free-text sources about why the person applied (classified into the reason categories). */
export const APPLICATION_TEXT_SOURCES = ['oubodouki', 'ouboriyuu_baitaikisai', 'ouboriyuu_hiaringu'];
/** Free text about leaving the current or last job. Shown and classified on its own, never mixed with application reasons. */
export const TRANSFER_TEXT_SOURCE = 'genshokumaeshokukaranotenshokuriyuu';
export const TEXT_SOURCES = [...APPLICATION_TEXT_SOURCES, TRANSFER_TEXT_SOURCE];
export const CATEGORY_SOURCES = ['ouboriyuukategori_hiaringu', 'ouboriyuukategori_baitaikisai'];
/** The sources of a stored file written before 2026-10-08. */
export const LEGACY_SOURCES = ['oubodouki', 'ouboriyuu_baitaikisai', 'ouboriyuu_hiaringu'];

/** Count source texts, never people, inferred motives, or causal reactions. */
export function reasonCohorts(collection: ApplicantReasonCollection | undefined, beforeId: string | undefined, afterId: string | undefined, sourceProperty = 'all') {
  if (!collection?.available) return null;
  const items = collection.items.filter(item => sourceProperty === 'all' || item.sourceProperty === sourceProperty);
  const known = collection.items.some(item => item.versionId !== null);
  return {
    before: items.filter(item => beforeId !== undefined && item.versionId === beforeId),
    after: items.filter(item => afterId !== undefined && item.versionId === afterId),
    unknown: items.filter(item => item.versionId === null),
    other: items.filter(item => item.versionId !== null && item.versionId !== beforeId && item.versionId !== afterId),
    versionAttributionAvailable: known,
    displayed: items.length,
  };
}
