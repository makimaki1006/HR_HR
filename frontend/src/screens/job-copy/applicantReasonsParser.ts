import type { ApplicantReasonCollection, ReasonSourceCounts } from './applicantReasonsModel';
import { reasonSourceLabels } from './applicantReasonsModel';

const invalid = (): never => { throw new Error('応募理由の取得データを確認できませんでした。'); };
function object(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return invalid();
  return value as Record<string, unknown>;
}
function only(value: Record<string, unknown>, keys: string[]) {
  if (Object.keys(value).some(key => !keys.includes(key))) invalid();
}
function count(value: unknown): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) return invalid();
  return value;
}
function timestamp(value: unknown): string {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}T/.test(value) || !Number.isFinite(Date.parse(value))) return invalid();
  return value;
}
// Backend bound is Unicode scalar count (Rust chars), not grapheme count.
function scalarCount(value: string): number { let total = 0; for (const scalar of value) total += scalar ? 1 : 0; return total; }
function date(value: unknown): string | null {
  if (value === null) return null;
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return invalid();
  const parsed = new Date(`${value}T00:00:00Z`);
  if (!Number.isFinite(parsed.getTime()) || parsed.toISOString().slice(0, 10) !== value) return invalid();
  return value;
}

/** Opaque keys and explicitly supplied cohort IDs only; never infer a version from dates. */
export function parseApplicantReasons(value: unknown, totalApplicants: number, publishedVersionIds: string[]): ApplicantReasonCollection | undefined {
  if (value === undefined || value === null) return undefined;
  const raw = object(value);
  only(raw, ['available', 'source', 'basis', 'source_property', 'fetched_at', 'total_applicants', 'total_source_values', 'source_counts', 'items', 'missing', 'blank', 'truncated']);
  if (raw.available !== true || raw.source !== 'hubspot' || raw.basis !== 'recorded_applicant_reason' || raw.source_property !== null || typeof raw.truncated !== 'boolean') return invalid();
  if (count(raw.total_applicants) !== totalApplicants || count(raw.total_source_values) !== totalApplicants * 3 || !Array.isArray(raw.items) || raw.items.length > 100) return invalid();
  const sourceCounts: Record<string, ReasonSourceCounts> = {};
  const sourceRaw = object(raw.source_counts);
  only(sourceRaw, Object.keys(reasonSourceLabels));
  for (const property of Object.keys(reasonSourceLabels)) {
    const counts = object(sourceRaw[property]);
    only(counts, ['missing', 'blank', 'nonblank']);
    const parsed = { missing: count(counts.missing), blank: count(counts.blank), nonblank: count(counts.nonblank) };
    if (parsed.missing + parsed.blank + parsed.nonblank !== totalApplicants) return invalid();
    sourceCounts[property] = parsed;
  }
  const sources = Object.values(sourceCounts);
  if (sources.reduce((n, row) => n + row.missing, 0) !== count(raw.missing) || sources.reduce((n, row) => n + row.blank, 0) !== count(raw.blank)) return invalid();
  const seen = new Set<string>();
  const items = raw.items.map(value => {
    const item = object(value);
    only(item, ['id', 'text', 'source', 'source_property', 'application_date', 'collected_at', 'version_id']);
    if (typeof item.id !== 'string' || !/^[a-f0-9]{64}$/.test(item.id) || seen.has(item.id) || item.source !== 'hubspot'
      || typeof item.source_property !== 'string' || !Object.hasOwn(reasonSourceLabels, item.source_property)
      || typeof item.text !== 'string' || !item.text.trim() || scalarCount(item.text) > 2000) return invalid();
    seen.add(item.id);
    if (item.version_id !== null && (typeof item.version_id !== 'string' || !publishedVersionIds.includes(item.version_id))) return invalid();
    return { id: item.id, text: item.text, sourceProperty: item.source_property,
      applicationDate: date(item.application_date), collectedAt: item.collected_at === null ? null : timestamp(item.collected_at), versionId: item.version_id };
  });
  const nonblank = sources.reduce((n, row) => n + row.nonblank, 0);
  if (items.length > nonblank || (!raw.truncated && items.length !== nonblank)) return invalid();
  for (const [property, counts] of Object.entries(sourceCounts)) if (items.filter(item => item.sourceProperty === property).length > counts.nonblank) return invalid();
  return { available: true, basis: raw.basis, fetchedAt: timestamp(raw.fetched_at), totalApplicants,
    totalSourceValues: count(raw.total_source_values), sourceCounts, missing: count(raw.missing), blank: count(raw.blank), truncated: raw.truncated, items };
}
