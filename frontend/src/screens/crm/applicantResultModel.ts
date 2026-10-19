import type { ApplicantDetail } from '../../generated/ApplicantDetail';
import type { ApplicantEditField } from '../../generated/ApplicantEditField';
import { appendMemo, todayJst } from './callResultModel';
import { fieldDefFrom, initialDraft } from './writeModel';
import type { FieldChange } from './writeModel';
export const APPLICANT_DRAFT_KEY = 'hrhr.crm.applicantResult.v1';
export const APPLICANT_MEMO = 'renrakukakuninmokuteki';
export const APPLICANT_WRITES_OFF = '応募の書き込みは OFF のため、読み取りのみです。';
export function applicantField(f: ApplicantEditField) { return fieldDefFrom('deal', { ...f, type: f.property_type }); }
export function applicantChanges(detail: ApplicantDetail, draft: Record<string, string>, stage: string, today = todayJst()): FieldChange[] {
  const changes: FieldChange[] = [];
  for (const field of detail.editable) {
    const def = applicantField(field);
    const entered = draft[field.name];
    if (!def || entered === undefined) continue;
    const base = detail.values[field.name] ?? null;
    const value = field.name === APPLICANT_MEMO ? appendMemo(base, entered, 'textarea', today)
      : field.property_type === 'datetime' ? (entered ? new Date(`${entered}+09:00`).toISOString() : null) : entered || null;
    if (field.name === APPLICANT_MEMO && value === null) continue;
    if (field.property_type === 'datetime' ? datetimeInput(base) === entered : (base ?? '') === (value ?? '')) continue;
    changes.push({ object: 'deal', name: field.name, base, value });
  }
  if (stage && stage !== detail.row.stage_id) changes.push({ object: 'deal', name: 'hs_pipeline_stage', base: detail.row.stage_id, value: stage });
  return changes;
}
export function datetimeInput(raw: string | null | undefined) {
  if (!raw) return '';
  const ms = /^\d{10,}$/.test(raw) ? Number(raw) : Date.parse(raw);
  return Number.isFinite(ms) ? new Date(ms + 9 * 3600_000).toISOString().slice(0, 16) : '';
}
export function applicantInput(field: ApplicantEditField, value: string | null | undefined) {
  const def = applicantField(field);
  return field.property_type === 'datetime' ? datetimeInput(value) : def ? initialDraft(def.kind, value) : '';
}
