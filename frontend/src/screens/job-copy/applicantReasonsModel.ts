export interface ApplicantReason {
  id: string;
  text: string;
  sourceProperty: string;
  applicationDate: string | null;
  collectedAt: string | null;
  versionId: string | null;
}
export interface ReasonSourceCounts { missing: number; blank: number; nonblank: number }
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
}

export const reasonSourceLabels: Record<string, string> = {
  oubodouki: '応募動機（HubSpot）',
  ouboriyuu_baitaikisai: '応募理由（媒体記載）',
  ouboriyuu_hiaringu: '応募理由（ヒアリング）',
};

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
