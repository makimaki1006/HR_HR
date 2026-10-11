import columns from '../../generated/jobgen/columns.json';
import type { CopyVersion, JobCopyRecord } from './data';
import { parseHrhFields, composeSalary } from './hrhCopy';
import { comparisonLabel, comparisonValue } from './dealComparisonModel';
import { displayCode, holidayValue, welfareValue } from './copyDisplay';
import { imagesByVersion } from './images';
import { publicationLabel } from './hubspotListings';

export interface ListingEntry { key: string; label: string; value: string }
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
  const row = Object.fromEntries(fields);
  const get = (key: string) => fields.get(key) ?? '';
  const entry = (key: string): ListingEntry => ({ key, label: comparisonLabel(key), value: comparisonValue(key, get(key), raw, row) });
  const salary = ['給与形態', '基本給与 最小', '基本給与 最大'].some(key => get(key).trim())
    ? composeSalary(get('給与形態').trim(), get('基本給与 最小').trim(), get('基本給与 最大').trim()) : '未取得';
  const consumed = new Set(['案件名', '公開', ...imageKeys]);
  const sections: ListingSection[] = groups.map(([heading, keys]) => {
    keys.forEach(key => consumed.add(key));
    let entries = keys.filter(key => get(key).trim()).map(entry);
    if (heading === '給与') {
      entries = entries.filter(item => !['給与形態', '基本給与 最小', '基本給与 最大'].includes(item.key));
      if (salary !== '未取得') entries.unshift({ key: 'salary', label: '給与', value: salary });
    }
    return { heading, entries };
  }).filter(section => section.entries.length);
  const other = [...fields.keys()].filter(key => !consumed.has(key) && !isManagement(key) && get(key).trim()).map(entry);
  if (other.length) sections.push({ heading: 'その他', entries: other });
  const fieldImages = imageKeys.flatMap((key, index) => {
    const url = get(key).trim();
    return /^https?:\/\//i.test(url) ? [{ id: key, url, caption: `画像${String(index + 1)}` }] : [];
  });
  const images = (version ? version.images ?? imagesByVersion[version.id] : undefined) ?? (fieldImages.length ? fieldImages : undefined);
  // Non-URL image values are retained as an acquisition note, never discarded.
  const unresolvedImages = imageKeys.filter(key => get(key).trim() && !/^https?:\/\//i.test(get(key).trim())).map(() => ({ key: 'image-note', label: '掲載画像の案内', value: '確認が必要' }));
  if (unresolvedImages.length) sections.push({ heading: '画像の案内', entries: unresolvedImages.map((item, i) => ({ ...item, key: `image-note-${String(i)}` })) });
  // Only add separately observed current images to the latest body (or no body).
  const observedImages = (current || !version) && images === undefined ? job.currentImageObservation : undefined;
  const publication = get('公開').trim() ? displayCode('公開', get('公開')) === '確認が必要' ? '確認が必要' : publicationLabel(get('公開'))
    : version?.observedPublicationStatus ? publicationLabel(version.observedPublicationStatus) : (current || !version) ? publicationLabel(job.listingPublicationStatus ?? null) : '未取得';
  const title = get('案件名').trim() || job.title;
  const location = get('勤務地').trim() || ((current || !version) ? job.location : '') || '未取得';
  // A summary value can come from independently acquired listing data.
  const summaryValues: Record<string, string> = { 求人名: title, 公開状況: publication, 勤務地: location, 給与: salary };
  const missingKeys = [...new Set([...columns.filter(key => !isManagement(key) && !imageKeys.includes(key)), '勤務地', '休日・休暇', '福利厚生', ...fields.keys()])];
  const missing = missingKeys.filter(key => {
    const summary = summaryValues[comparisonLabel(key)];
    return !isManagement(key) && !imageKeys.includes(key) && !get(key).trim() && !(summary && !['未取得', '求人名未取得'].includes(summary));
  }).map(comparisonLabel);
  if (images === undefined && !observedImages) missing.push('画像');
  const total = (current || !version) ? job.overallApplications?.total ?? job.listingApplicationCount : undefined;
  return { sections, missing, images, observedImages, hasCurrentConditions, salary, publication,
    title, location,
    applications: total == null ? '未取得' : `${String(total)}件`,
  };
}
