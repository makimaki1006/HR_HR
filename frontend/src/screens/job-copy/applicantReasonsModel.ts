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
  /**
   * How the names of the chosen categories were obtained, which tells why a chosen value has no
   * name: 'read' (the option list was read and does not hold the value: the option was removed
   * or renamed), 'unavailable' (the option list could not be read just now; reopening may help),
   * 'not_stored' (a stored file without the names; reopening does not add them). null when not
   * said (a stored file written before this was recorded): treated like 'not_stored'.
   */
  optionLabels: OptionLabelsStatus | null;
  /**
   * Applicant keys of the applications HubSpot also links to another job. Their reasons are left
   * out of the reason counts, as their applications are left out of the period table. Absent or
   * null: not read (a stored file, or the link read failed), so they cannot be told apart.
   */
  multiListingApplicants?: readonly string[] | null;
}
export type OptionLabelsStatus = 'read' | 'unavailable' | 'not_stored';

/** Every source, in the order the server reads them. Shown names are plain words. */
export const reasonSourceLabels: Record<string, string> = {
  oubodouki: '応募動機（HubSpot）',
  ouboriyuu_baitaikisai: '応募理由（媒体記載）',
  ouboriyuu_hiaringu: '応募理由（ヒアリング）',
  genshokumaeshokukaranotenshokuriyuu: '今の仕事・前の仕事から転職する理由',
  ouboriyuukategori_hiaringu: '応募理由の分類（ヒアリング）',
  ouboriyuukategori_baitaikisai: '応募理由の分類（媒体記載）',
  hoyuushikaku_hiaringu: '保有資格',
  kanrengyoumukeiken: '関連業務経験',
  shuugyoujoukyou: '就業状況',
  kinmujoukennoyuusenjuni: '勤務条件の優先順位',
  oubonoheiganjoukyou: '併願状況',
  kyuujinkensakujouken: '求人の探し方（検索条件）',
  shigotosagashinohanikoutsuushudan: '仕事探しの範囲・交通手段',
  nyuushakiboujiki: '入社希望時期',
  ichijitaiou_azukatta_shitsumon: '預かった質問',
  ichijitaiou_jitai_riyuu: '辞退理由',
  ichijitaiou_shokan_memo: '所感メモ',
  tokukijikou: '特記事項',
  bikou_hiaringu: '備考',
  call_notes: '架電メモ',
};
/** Free-text sources about why the person applied (classified into the reason categories). */
export const APPLICATION_TEXT_SOURCES = ['oubodouki', 'ouboriyuu_baitaikisai', 'ouboriyuu_hiaringu'];
/** Free text about leaving the current or last job. Shown and classified on its own, never mixed with application reasons. */
export const TRANSFER_TEXT_SOURCE = 'genshokumaeshokukaranotenshokuriyuu';
export const HEARING_TEXT_SOURCES = ['hoyuushikaku_hiaringu', 'kanrengyoumukeiken', 'shuugyoujoukyou', 'kinmujoukennoyuusenjuni', 'oubonoheiganjoukyou', 'kyuujinkensakujouken', 'shigotosagashinohanikoutsuushudan', 'nyuushakiboujiki', 'ichijitaiou_azukatta_shitsumon', 'ichijitaiou_jitai_riyuu', 'ichijitaiou_shokan_memo', 'tokukijikou', 'bikou_hiaringu', 'call_notes'];
export const HEARING_CLASSIFICATION_SOURCES = ['kinmujoukennoyuusenjuni', 'oubonoheiganjoukyou', 'ichijitaiou_shokan_memo', 'bikou_hiaringu', 'call_notes'];
export const TEXT_SOURCES = [...APPLICATION_TEXT_SOURCES, TRANSFER_TEXT_SOURCE, ...HEARING_TEXT_SOURCES];
export const CATEGORY_SOURCES = ['ouboriyuukategori_hiaringu', 'ouboriyuukategori_baitaikisai'];
/** The sources of a stored file written before 2026-10-08. */
export const LEGACY_SOURCES = ['oubodouki', 'ouboriyuu_baitaikisai', 'ouboriyuu_hiaringu'];

/** Count source texts, never people, inferred motives, or causal reactions. */
export function reasonCohorts(collection: ApplicantReasonCollection | undefined, beforeId: string | undefined, afterId: string | undefined, sourceProperty = 'all') {
  if (!collection?.available) return null;
  // 'all' means every application-reason source. Transfer reasons are a different question and are only shown when chosen on their own.
  const items = collection.items.filter(item => sourceProperty === 'all' ? APPLICATION_TEXT_SOURCES.includes(item.sourceProperty) : item.sourceProperty === sourceProperty);
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
