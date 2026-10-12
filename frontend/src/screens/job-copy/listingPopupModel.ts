import columns from '../../generated/jobgen/columns.json';
import type { CopyVersion, JobCopyRecord } from './data';
import { parseHrhFields, composeSalary } from './hrhCopy';
import { comparisonLabel, comparisonValue } from './dealComparisonModel';
import { displayCode, holidayValue, welfareValue } from './copyDisplay';
import { imagesByVersion } from './images';
import { publicationLabel } from './hubspotListings';

export interface ListingEntry { key: string; label: string; value: string; unavailable?: boolean }
export interface ListingSection { heading: string; entries: ListingEntry[] }
const isManagement = (key: string) => /id$/i.test(key);
const imageKeys = ['画像1', '画像2', '画像3'];
const guides = Array.from({ length: 4 }, (_, i) => [`仕事情報補足${String(i + 1)}のタイトル`, `仕事情報補足${String(i + 1)}の内容`]);
const salaryKeys = columns.filter(key => /給与|固定残業代|試用・研修/.test(key));
const groups: [string, string[]][] = [
  ['仕事内容', ['仕事内容']], ['給与', salaryKeys],
  ['勤務地・アクセス', ['勤務地', '通勤経路', '最寄り駅']],
  ['勤務時間', ['勤務時間', '勤務時間帯', 'タスクの所要時間', 'タスクの単位', '平均稼働時間', '平均稼働日数', '想定残業時間']],
  ['休日・休暇', ['休日・休暇']], ['応募資格', ['応募資格']], ['福利厚生', ['福利厚生']],
  ...guides.map((keys, i): [string, string[]] => [`仕事の案内 ${String(i + 1)}`, keys]),
];
/** The selected body is authoritative. Current independent conditions are only
 * appended to the latest published body, with an explicit current-value note. */
export function listingPopupModel(job: JobCopyRecord, version?: CopyVersion) {
  const latest = job.versions.filter(item => item.kind === 'published').at(-1);
  const current = version !== undefined && version.id === latest?.id;
  const raw = version?.comparisonBody ?? version?.body ?? '';
  const fields = version?.draft ? new Map(Object.entries(version.draft.row)) : version?.comparisonBody !== undefined ? parseHrhFields(raw) : new Map<string, string>();
  if (!fields.size && raw.trim()) fields.set('仕事内容', raw);
  const hasCurrentConditions = (current || !version) && job.currentListingFields !== undefined;
  if (hasCurrentConditions) for (const [key, value] of Object.entries(job.currentListingFields ?? {})) {
    // Never replace a body's own value, including explicitly empty values.
    if (!fields.has(key)) fields.set(key, value);
  }
  const holiday = holidayValue(fields);
  if (holiday && !fields.has('休日・休暇')) fields.set('休日・休暇', holiday);
  const welfare = welfareValue(fields);
  if (welfare && !fields.get('福利厚生')?.trim()) fields.set('福利厚生', welfare);
  if ((current || !version) && !fields.has('勤務地') && job.location) fields.set('勤務地', job.location);
  const bodyAvailable = Boolean(raw.trim());
  const structured = version?.draft !== undefined || columns.every(key => fields.has(key));
  const row = Object.fromEntries(fields);
  const get = (key: string) => fields.get(key) ?? '';
  const entry = (key: string): ListingEntry => ({ key, label: comparisonLabel(key), value: comparisonValue(key, get(key), raw, row) });
  const hasSalary = ['給与形態', '基本給与 最小', '基本給与 最大'].some(key => get(key).trim());
  const salary = hasSalary ? composeSalary(get('給与形態').trim(), get('基本給与 最小').trim(), get('基本給与 最大').trim())
    : bodyAvailable && (structured || ['給与形態', '基本給与 最小', '基本給与 最大'].every(key => fields.has(key))) ? '記載なし' : '取得できていません';
  const consumed = new Set(['案件名', '公開', ...imageKeys]);
  const sections: ListingSection[] = groups.map(([heading, keys]) => {
    keys.forEach(key => consumed.add(key));
    let entries = keys.filter(key => get(key).trim()).map(entry);
    if (heading === '給与') {
      entries = entries.filter(item => !['給与形態', '基本給与 最小', '基本給与 最大'].includes(item.key));
      if (hasSalary) entries.unshift({ key: 'salary', label: '給与', value: salary });
    }
    if (groups.findIndex(group => group[0] === heading) < 7 && (heading === '給与' ? !hasSalary : !get(keys[0] ?? '').trim())) {
      const known = bodyAvailable && (heading === '勤務地・アクセス' ? fields.has('勤務地') : structured || fields.has(keys[0] ?? ''));
      entries.unshift({ key: heading === '給与' ? 'salary' : keys[0] ?? heading, label: heading, value: known ? '記載なし' : '取得できていません', unavailable: !known });
    }
    return { heading, entries };
  }).filter(section => section.entries.length);
  const other = [...fields.keys()].filter(key => !consumed.has(key) && !isManagement(key) && get(key).trim()).map(entry);
  if (other.length) sections.push({ heading: 'その他', entries: other });
  const fieldImages = imageKeys.flatMap((key, index) => {
    const url = get(key).trim();
    return /^https?:\/\//i.test(url) ? [{ id: key, url, caption: `画像${String(index + 1)}` }] : [];
  });
  const recordedImages = version ? version.images ?? imagesByVersion[version.id] : undefined;
  const images = recordedImages?.length ? recordedImages : fieldImages.length ? fieldImages : undefined;
  // Empty history arrays can mean that filenames were discarded, not no images.
  const observedImages = (current || !version) && !images?.length && job.currentImageObservation?.images.length ? job.currentImageObservation : undefined;
  const registered = current || !version ? job.currentImagesRegistered ?? version?.imagesRegistered : version.imagesRegistered;
  const imageValuesKnown = bodyAvailable && structured && imageKeys.every(key => fields.has(key));
  const hasImageReferences = imageKeys.some(key => !['', 'なし', '画像なし', '未登録'].includes(get(key).trim())) || registered === true;
  const imageSignature = imageValuesKnown ? JSON.stringify(imageKeys.map(key => get(key).trim()).filter(value => !['', 'なし', '画像なし', '未登録'].includes(value))) : recordedImages?.length ? JSON.stringify(recordedImages.map(image => image.sourceReferenceHash ?? image.url)) : registered === false ? '[]' : undefined;
  const imageReason = images?.length || observedImages ? undefined : hasImageReferences ? '画像はありますが、取得できていません' : imageValuesKnown || registered === false ? 'この求人には画像が登録されていません' : '画像の登録状況を取得できていません';
  const publication = get('公開').trim() ? displayCode('公開', get('公開')) === '確認が必要' ? '確認が必要' : publicationLabel(get('公開'))
    : version?.observedPublicationStatus ? publicationLabel(version.observedPublicationStatus) : (current || !version) ? publicationLabel(job.listingPublicationStatus ?? null) : '未取得';
  const title = get('案件名').trim() || job.title;
  const location = get('勤務地').trim() || ((current || !version) ? job.location : '') || (bodyAvailable && fields.has('勤務地') ? '記載なし' : '取得できていません');
  const mainKeys = new Set(['案件名', '公開', '仕事内容', '勤務地', '勤務時間', '応募資格', '休日・休暇', '福利厚生', '給与形態', '基本給与 最小', '基本給与 最大', ...imageKeys]);
  const missing = bodyAvailable && structured ? columns.filter(key => !isManagement(key) && !mainKeys.has(key) && !get(key).trim()).map(comparisonLabel) : [];
  const total = (current || !version) ? job.overallApplications?.total ?? job.listingApplicationCount : undefined;
  return { sections, missing, images, observedImages, imageReason, imageSignature, bodyAvailable, structured, hasCurrentConditions, salary, publication,
    title, location,
    applications: total == null ? '未取得' : `${String(total)}件`,
  };
}

export function listingComparisonRows(current: ReturnType<typeof listingPopupModel>, previous?: ReturnType<typeof listingPopupModel>) {
  const flatten = (model: ReturnType<typeof listingPopupModel>) => model.sections.flatMap(section => section.entries.map(entry => ({ ...entry, heading: section.heading })));
  const left = flatten(current), right = previous ? flatten(previous) : [];
  const keys = [...new Set([...left.map(item => item.key), ...right.map(item => item.key)])];
  return keys.map(key => {
    const a = left.find(item => item.key === key), b = right.find(item => item.key === key);
    const label = a?.label ?? b?.label ?? 'その他の案内';
    const value = a?.value ?? (current.bodyAvailable && current.structured ? '記載なし' : '取得できていません');
    const before = b?.value ?? (previous?.bodyAvailable && previous.structured ? '記載なし' : '取得できていません');
    const different = Boolean(previous && current.bodyAvailable && previous.bodyAvailable && !a?.unavailable && !b?.unavailable && value !== '取得できていません' && before !== '取得できていません' && value !== before);
    return { key, label, heading: a?.heading ?? b?.heading, value, before, different };
  });
}
