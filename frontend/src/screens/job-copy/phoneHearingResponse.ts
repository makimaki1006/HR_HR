import type { PhoneHearing } from '../../generated/PhoneHearing';
const object = (value: unknown): Record<string, unknown> | null => typeof value === 'object' && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : null;
const text = (value: unknown): value is string => typeof value === 'string' && value.trim().length > 0 && Array.from(value).length <= 2000;
const nullableText = (value: unknown) => value === null || text(value);

/** A malformed or partial reply is a failed read, never zero records. */
export function isPhoneHearing(value: unknown): value is PhoneHearing {
  const raw = object(value);
  if (!raw || !Number.isSafeInteger(raw.total_applicants) || Number(raw.total_applicants) < 0 || typeof raw.truncated !== 'boolean' || !Array.isArray(raw.items) || raw.items.length > 500 || raw.items.length > Number(raw.total_applicants)) return false;
  const seen = new Set<string>();
  return raw.items.every((item: unknown) => {
    const card = object(item);
    if (!card || typeof card.applicant !== 'string' || !/^[a-f0-9]{64}$/.test(card.applicant) || seen.has(card.applicant) || !nullableText(card.media) || typeof card.multi_listing !== 'boolean') return false;
    if (['gender', 'prefecture', 'municipality'].some(key => card[key] !== undefined && !nullableText(card[key]))) return false;
    if (card.age !== undefined && card.age !== null && (typeof card.age !== 'string' || !/^\d+$/.test(card.age))) return false;
    seen.add(card.applicant);
    if (card.application_date !== null && (typeof card.application_date !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(card.application_date))) return false;
    if (!Array.isArray(card.fields) || !Array.isArray(card.notes) || card.fields.length + card.notes.length === 0) return false;
    return card.fields.every((item: unknown) => { const field = object(item); return field && text(field.label) && text(field.text); })
      && card.notes.every((item: unknown) => { const note = object(item); return note && text(note.text) && (note.time === null || (typeof note.time === 'string' && Number.isFinite(Date.parse(note.time)))); });
  });
}
