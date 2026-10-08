import type { ApplicantReasonCollection, OptionLabelsStatus, ReasonSelection, ReasonSourceCounts } from './applicantReasonsModel';
import { CATEGORY_SOURCES, LEGACY_SOURCES, TEXT_SOURCES, reasonSourceLabels } from './applicantReasonsModel';
import { maskPersonalDetails } from './personalText';

const OPTION_LABEL_STATUSES: OptionLabelsStatus[] = ['read', 'unavailable', 'not_stored'];
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
function opaque(value: unknown): boolean { return typeof value === 'string' && /^[a-f0-9]{64}$/.test(value); }
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
  only(raw, ['available', 'source', 'basis', 'source_property', 'fetched_at', 'total_applicants', 'total_source_values', 'source_counts', 'items', 'selections', 'option_labels', 'multi_listing_applicants', 'missing', 'blank', 'truncated']);
  if (raw.available !== true || raw.source !== 'hubspot' || raw.basis !== 'recorded_applicant_reason' || raw.source_property !== null || typeof raw.truncated !== 'boolean') return invalid();
  const sourceRaw = object(raw.source_counts);
  // A stored file written before 2026-10-08 has the three old sources, no applicant keys and no
  // selections; the other sources are then 未取得 (absent), never 0.
  const legacy = Object.keys(sourceRaw).length === LEGACY_SOURCES.length;
  const sourceKeys = legacy ? LEGACY_SOURCES : Object.keys(reasonSourceLabels);
  const textSources = legacy ? LEGACY_SOURCES : TEXT_SOURCES;
  if (legacy !== (raw.selections === undefined)) return invalid();
  // Not in a stored file written before 2026-10-08 (no selections there); optional otherwise.
  if (legacy && raw.option_labels !== undefined) return invalid();
  if (raw.option_labels !== undefined && !OPTION_LABEL_STATUSES.includes(raw.option_labels as OptionLabelsStatus)) return invalid();
  const optionLabels = raw.option_labels === undefined ? null : raw.option_labels as OptionLabelsStatus;
  if (count(raw.total_applicants) !== totalApplicants || count(raw.total_source_values) !== totalApplicants * sourceKeys.length || !Array.isArray(raw.items) || raw.items.length > 500) return invalid();
  const sourceCounts: Record<string, ReasonSourceCounts> = {};
  only(sourceRaw, sourceKeys);
  for (const property of sourceKeys) {
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
    only(item, ['id', 'applicant', 'text', 'source', 'source_property', 'application_date', 'collected_at', 'version_id']);
    if (legacy !== (item.applicant === undefined) || (!legacy && !opaque(item.applicant))) return invalid();
    if (typeof item.id !== 'string' || !/^[a-f0-9]{64}$/.test(item.id) || seen.has(item.id) || item.source !== 'hubspot'
      || typeof item.source_property !== 'string' || !textSources.includes(item.source_property)
      || typeof item.text !== 'string' || !item.text.trim() || scalarCount(item.text) > 2000) return invalid();
    seen.add(item.id);
    if (item.version_id !== null && (typeof item.version_id !== 'string' || !publishedVersionIds.includes(item.version_id))) return invalid();
    return { id: item.id, applicant: legacy ? null : item.applicant as string, text: maskPersonalDetails(item.text), sourceProperty: item.source_property,
      applicationDate: date(item.application_date), collectedAt: item.collected_at === null ? null : timestamp(item.collected_at), versionId: item.version_id };
  });
  const nonblank = textSources.reduce((n, property) => n + (sourceCounts[property]?.nonblank ?? 0), 0);
  if (items.length > nonblank || (!raw.truncated && items.length !== nonblank)) return invalid();
  for (const property of textSources) if (items.filter(item => item.sourceProperty === property).length > (sourceCounts[property]?.nonblank ?? 0)) return invalid();
  let selections: ReasonSelection[] | null = null;
  if (!legacy) {
    if (!Array.isArray(raw.selections)) return invalid();
    const chosen = new Set<string>();
    selections = raw.selections.map(value => {
      const selection = object(value);
      only(selection, ['applicant', 'source_property', 'value', 'label', 'application_date']);
      if (!opaque(selection.applicant) || typeof selection.source_property !== 'string' || !CATEGORY_SOURCES.includes(selection.source_property)
        || typeof selection.value !== 'string' || !selection.value.trim() || scalarCount(selection.value) > 100
        || (selection.label !== null && (typeof selection.label !== 'string' || !selection.label.trim() || scalarCount(selection.label) > 100))) return invalid();
      const key = `${selection.applicant as string}\u0000${selection.source_property}\u0000${selection.value}`;
      if (chosen.has(key)) return invalid();
      chosen.add(key);
      return { applicant: selection.applicant as string, sourceProperty: selection.source_property, value: maskPersonalDetails(selection.value),
        label: selection.label === null ? null : maskPersonalDetails(selection.label), applicationDate: date(selection.application_date) };
    });
    for (const property of CATEGORY_SOURCES) {
      const applicants = new Set(selections.filter(row => row.sourceProperty === property).map(row => row.applicant));
      if (applicants.size !== sourceCounts[property]?.nonblank) return invalid();
    }
  }
  // Only in the current shape (applicant keys); absent when the links to other jobs were not read.
  let multiListingApplicants: string[] | null = null;
  if (raw.multi_listing_applicants !== undefined) {
    const keys = raw.multi_listing_applicants;
    if (legacy || !Array.isArray(keys) || keys.length > totalApplicants || !keys.every(opaque) || new Set(keys).size !== keys.length) return invalid();
    multiListingApplicants = keys as string[];
  }
  return { available: true, basis: raw.basis, fetchedAt: timestamp(raw.fetched_at), totalApplicants,
    totalSourceValues: count(raw.total_source_values), sourceCounts, missing: count(raw.missing), blank: count(raw.blank), truncated: raw.truncated, items, selections, optionLabels, multiListingApplicants };
}
