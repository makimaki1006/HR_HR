/**
 * Application reasons sorted into the categories of the HubSpot select 応募理由カテゴリ
 * (給与 / 勤務地 / 職種興味 / 会社規模 / その他). Pure functions only.
 *
 * - A category chosen in HubSpot counts as 「選択済み」.
 * - An application with no chosen category but with a reason text (応募動機・応募理由) is sorted by
 *   the keyword dictionary below into one or more categories, marked 「キーワードで推定」.
 * - A text that matches no keyword is 「分類できない」.
 * 選択済み and 推定 are always counted apart. The texts about leaving the current or last job
 * (転職理由) are classified on their own and never mixed into the application reasons.
 *
 * Counts are applications (件), not people. Nothing here says why an application was made; it only
 * sorts what was recorded. The dictionary is also written out in
 * claudedocs/JOB_COPY_REASONS_2026-10-08.md.
 */
import type { ApplicantReason, ApplicantReasonCollection, ReasonSelection } from './applicantReasonsModel';
import { APPLICATION_TEXT_SOURCES, TRANSFER_TEXT_SOURCE } from './applicantReasonsModel';

export const REASON_CATEGORIES = ['給与', '勤務地', '職種興味', '会社規模', 'その他'] as const;
export type ReasonCategory = (typeof REASON_CATEGORIES)[number];
/** The select option that means nothing was chosen. */
export const UNSET_LABEL = '未設定';
/** The most texts the server sends (applicant_reasons::MAX_ITEMS); more are left out (truncated). */
export const MAX_READ_TEXTS = 500;
/** How the shown reason texts were masked (maskPersonalDetails). One wording on every list of texts. */
export const MASK_NOTE = '市区町村より細かい住所（町名・番地・建物名と部屋番号・郵便番号）・電話番号・メールアドレス・LINEなどのID・生年月日・「さん」「様」などの付いた名前は、読み取れた範囲で「＊＊」に置き換えています。「さん」などの付かない名前（例: 姉の山田花子）や、読み取れない書き方の住所など、それ以外の個人情報が残っていることがあります。';
/** Below this many applications a share (%) is not shown, only the counts. */
export const MIN_SHARE_N = 5;

/**
 * Keyword dictionary (matched as substrings after NFKC normalization; one text can match several
 * categories). Words are chosen to be about the reason itself; a plain job name (介護, 配送 ...)
 * is not a keyword, since it appears in texts about anything.
 */
export const REASON_KEYWORDS: Readonly<Record<ReasonCategory, readonly string[]>> = {
  給与: ['給与', '給料', '時給', '月給', '日給', '年収', '月収', '収入', '賃金', '手当', '賞与', 'ボーナス', '昇給', '稼げ', '稼ぎ', '報酬', '歩合', 'インセンティブ'],
  勤務地: ['勤務地', '近い', '近く', '近所', '通勤', '通いやすい', '通える', '自宅から', '家から', '家の近', '駅から', '駅近', '徒歩', '地元', '転勤なし', '転勤がない', '引っ越', '引越', 'アクセス'],
  職種興味: ['仕事がしたい', '仕事をしたい', '仕事に就きたい', '職に就きたい', '興味', 'やってみたい', 'やりたい', 'してみたい', '活かし', '活かせ', '生かし', '生かせ', 'が好き', '好きな仕事', '好きだから', 'やりがい', '携わ', '挑戦', 'チャレンジ', '憧れ', '向いて', '続けたい', '経験がある', '経験あり'],
  会社規模: ['大手', '大企業', '安定した会社', '安定した企業', '安定企業', '会社が安定', '経営が安定', '経営の安定', '安定性', '規模', '上場', '有名', '知名度', '老舗', 'グループ会社', '全国展開'],
  その他: ['休み', '休日', '土日', '週休', 'シフト', '勤務時間', '短時間', '時短', '残業', '夜勤', '日勤', '扶養', '研修', '未経験', '雰囲気', '人間関係', '福利厚生', '社員寮', '社宅', '子育て', '育児', '託児', '両立', '正社員', '紹介', '服装', '髪型'],
};

/**
 * Phrases taken out of the text before the keywords are looked for, because a keyword inside them
 * means something else: 「好きな時間に働ける」 is about hours, not the job; 「定年が近い」 is not about
 * the place; 「大手スーパーより時給が低い」 compares with another company and is not about this
 * company's size. Shown on screen as written in EXCLUDED_PHRASE_NOTES.
 */
export const EXCLUDED_PHRASES: readonly RegExp[] = [
  /好きな(時間|日|曜日|時期|タイミング)/g,
  /(定年|年齢|年|歳|理想|希望)(が|に)近/g,
  /近い(うち|内|将来|未来|時期|日に)/g,
];
export const EXCLUDED_PHRASE_NOTES = ['「好きな時間・日・曜日」', '「定年・年齢・理想が近い」', '「近いうち・近い将来・近い未来・近い時期」', '「大手〜より・と比べ」（ほかの会社との比較。この中の会社の大きさの言葉だけ使わず、時給などの言葉は使います）'];
/**
 * A comparison with another company (「大手スーパーの時給より高い」). Only the 会社規模 words inside
 * it are taken out: the rest of the span (時給 ...) is still about this job.
 */
export const COMPARISON_PHRASE = /大手[^。、,.!?！？]*?(より|と比べ|に比べ)/g;

function normalize(text: string): string {
  return text.normalize('NFKC').toLowerCase();
}

/** The keywords normalized once (the dictionary never changes). */
const NORMALIZED_KEYWORDS: readonly (readonly [ReasonCategory, readonly string[]])[] = REASON_CATEGORIES.map(category => [category, REASON_KEYWORDS[category].map(normalize)] as const);

/** The 会社規模 keywords, longest first so 「大企業」 is taken out before a shorter word inside it. */
const SIZE_KEYWORDS: readonly string[] = [...(NORMALIZED_KEYWORDS.find(([category]) => category === '会社規模')?.[1] ?? [])].sort((a, b) => b.length - a.length);

/** Categories whose keywords appear in the text, in the fixed category order. */
export function inferCategories(text: string): ReasonCategory[] {
  const withoutPhrases = EXCLUDED_PHRASES.reduce((value, pattern) => value.replace(pattern, '／'), normalize(text));
  const normalized = withoutPhrases.replace(COMPARISON_PHRASE, span => SIZE_KEYWORDS.reduce((value, word) => value.split(word).join('／'), span));
  return NORMALIZED_KEYWORDS.filter(([, words]) => words.some(word => normalized.includes(word))).map(([category]) => category);
}

/** HubSpot の選択肢「未設定」の内部値 (2026-10-08 に定義を読んで確認。他の選択肢は内部値 = ラベル) */
const UNSET_VALUE = 'unset';

/** The category of a chosen value: its label (or the stored value) when it names a category, 'unset' for 未設定, null otherwise. */
export function selectedCategory(selection: Pick<ReasonSelection, 'value' | 'label'>): ReasonCategory | 'unset' | null {
  for (const name of [selection.label, selection.value]) {
    if (name === null) continue;
    const plain = normalize(name).trim();
    const category = REASON_CATEGORIES.find(item => normalize(item) === plain);
    if (category) return category;
    if (plain === normalize(UNSET_LABEL) || plain === UNSET_VALUE) return 'unset';
  }
  return null;
}

export type ReasonBasis = 'selected' | 'estimated' | 'unclassified';
export interface ClassifiedApplication {
  /** Opaque key (applicant key, or the text's key in a stored file without applicant keys). */
  key: string;
  applicationDate: string | null;
  basis: ReasonBasis;
  categories: ReasonCategory[];
  /** Masked texts that were used (応募動機・応募理由). */
  texts: ApplicantReason[];
  /** Chosen option names (labels) that name no category (shown as they are). */
  otherValues: string[];
  /**
   * Chosen values whose option name could not be read (the stored value is not shown, since it is
   * an internal code). Not counted as 選択済み.
   */
  unnamedSelections: number;
}
export interface Classification {
  applications: ClassifiedApplication[];
  /** 'text' when the stored file has no applicant keys: each text is counted on its own. */
  unit: 'application' | 'text';
  /** Applications whose category was 未設定 and that had no text. */
  unsetOnly: number;
  /**
   * Applications not counted as 選択済み that have a chosen value whose option name could not be read.
   * An application that also has a named category is 選択済み and is not counted here, since the
   * screen note says these applications were not counted as 選択済み.
   */
  unnamedApplications: number;
  /**
   * Applications with a recorded reason that HubSpot also links to another job, left out of
   * `applications` (as the period table leaves their applications out). null when that was not
   * read, or when texts are counted one by one: they may then be counted in.
   */
  multiListing: number | null;
}

function classifyGroup(key: string, texts: ApplicantReason[], selections: ReasonSelection[]): ClassifiedApplication | 'unset' | null {
  const applicationDate = [...texts, ...selections].map(row => row.applicationDate).find(value => value !== null) ?? null;
  const chosen = new Set<ReasonCategory>();
  const otherValues: string[] = [];
  let unnamedSelections = 0;
  let unset = false;
  for (const selection of selections) {
    const category = selectedCategory(selection);
    if (category === 'unset') unset = true;
    else if (category) chosen.add(category);
    else if (selection.label !== null) otherValues.push(selection.label);
    else unnamedSelections += 1;
  }
  if (chosen.size) return { key, applicationDate, basis: 'selected', categories: REASON_CATEGORIES.filter(category => chosen.has(category)), texts, otherValues, unnamedSelections };
  if (texts.length) {
    const found = new Set(texts.flatMap(item => inferCategories(item.text)));
    const categories = REASON_CATEGORIES.filter(category => found.has(category));
    return { key, applicationDate, basis: categories.length ? 'estimated' : 'unclassified', categories, texts, otherValues, unnamedSelections };
  }
  if (otherValues.length || unnamedSelections) return { key, applicationDate, basis: 'unclassified', categories: [], texts, otherValues, unnamedSelections };
  return unset ? 'unset' : null;
}

function classify(collection: ApplicantReasonCollection | undefined, sources: readonly string[], useSelections: boolean): Classification | null {
  if (!collection?.available) return null;
  const unit = collection.items.some(item => item.applicant === null) ? 'text' : 'application';
  const groups = new Map<string, { texts: ApplicantReason[]; selections: ReasonSelection[] }>();
  const group = (key: string) => { let found = groups.get(key); if (!found) { found = { texts: [], selections: [] }; groups.set(key, found); } return found; };
  for (const item of collection.items) if (sources.includes(item.sourceProperty)) group(item.applicant ?? item.id).texts.push(item);
  if (useSelections) for (const selection of collection.selections ?? []) group(selection.applicant).selections.push(selection);
  const applications: ClassifiedApplication[] = [];
  const multi = unit === 'application' && collection.multiListingApplicants ? new Set(collection.multiListingApplicants) : null;
  let unsetOnly = 0;
  let multiListing = 0;
  for (const [key, value] of groups) {
    const result = classifyGroup(key, value.texts, value.selections);
    if (result && multi?.has(key)) multiListing += 1;
    else if (result === 'unset') unsetOnly += 1;
    else if (result) applications.push(result);
  }
  return { applications, unit, unsetOnly, unnamedApplications: applications.filter(application => application.unnamedSelections > 0 && application.basis !== 'selected').length, multiListing: multi ? multiListing : null };
}

/** Application reasons: chosen categories first, then keywords in 応募動機・応募理由. null when not read. */
export function classifyApplicationReasons(collection: ApplicantReasonCollection | undefined): Classification | null {
  return classify(collection, APPLICATION_TEXT_SOURCES, true);
}
/** 転職理由 texts, by keywords only. null when not read (also in a stored file without that source). */
export function classifyTransferReasons(collection: ApplicantReasonCollection | undefined): Classification | null {
  if (!collection?.available || !collection.sourceCounts[TRANSFER_TEXT_SOURCE]) return null;
  return classify(collection, [TRANSFER_TEXT_SOURCE], false);
}

export interface CategoryCount { category: ReasonCategory; selected: number; estimated: number; total: number }
export interface ReasonTally {
  /** Applications with a recorded reason (選択済み + 推定 + 分類できない). */
  n: number;
  selectedN: number;
  estimatedN: number;
  unclassified: number;
  counts: CategoryCount[];
}

export function tally(applications: readonly ClassifiedApplication[]): ReasonTally {
  const counts = REASON_CATEGORIES.map(category => ({ category, selected: 0, estimated: 0, total: 0 }));
  let selectedN = 0; let estimatedN = 0; let unclassified = 0;
  for (const application of applications) {
    if (application.basis === 'selected') selectedN += 1;
    else if (application.basis === 'estimated') estimatedN += 1;
    else unclassified += 1;
    for (const category of application.categories) {
      const row = counts.find(item => item.category === category);
      if (!row) continue;
      if (application.basis === 'selected') row.selected += 1; else row.estimated += 1;
      row.total += 1;
    }
  }
  return { n: applications.length, selectedN, estimatedN, unclassified, counts };
}

/** The most frequent categories (ties keep the category order); categories with no application are left out. */
export function topReasons(result: ReasonTally, limit = 2): CategoryCount[] {
  return result.counts.filter(row => row.total > 0)
    .map((row, index) => ({ row, index }))
    .sort((a, b) => b.row.total - a.row.total || a.index - b.index)
    .slice(0, limit)
    .map(item => item.row);
}

/**
 * Like topReasons, plus how many more categories have the same count as the last one shown and
 * were cut off by the limit (a tie is never cut silently).
 */
export function topReasonsWithTies(result: ReasonTally, limit = 2): { rows: CategoryCount[]; tiedOut: number } {
  const all = topReasons(result, REASON_CATEGORIES.length);
  const rows = all.slice(0, limit);
  const last = rows.at(-1);
  return { rows, tiedOut: last ? all.slice(limit).filter(row => row.total === last.total).length : 0 };
}

/** 「42%」 when n is large enough, otherwise null (counts only). */
export function shareText(count: number, n: number): string | null {
  return n >= MIN_SHARE_N && n > 0 ? `${String(Math.round(count / n * 100))}%` : null;
}

/** 「選択2件・推定1件」: the two kinds of count of one category, never only their sum. */
export function basisText(row: CategoryCount): string {
  return [row.selected ? `選択${String(row.selected)}件` : '', row.estimated ? `推定${String(row.estimated)}件` : ''].filter(Boolean).join('・');
}

/** 「給与 3件（選択2件・推定1件）」, with the share (「・50%」) when n is large enough and n is given. */
export function categoryCountText(row: CategoryCount, n?: number): string {
  const share = n === undefined ? null : shareText(row.total, n);
  const parts = basisText(row);
  return `${row.category} ${String(row.total)}件${share ? `・${share}` : ''}${parts ? `（${parts}）` : ''}`;
}

export interface PeriodSpan { key: string; start: string; end: string }
export interface PeriodReasons { key: string; tally: ReasonTally }
/**
 * Reasons per period by application date. A period counts applications dated in [start, end).
 * Applications without a date, or dated outside every period, are counted apart.
 */
export function reasonsByPeriod(applications: readonly ClassifiedApplication[], spans: readonly PeriodSpan[]): { periods: PeriodReasons[]; undated: number; outside: number } {
  const periods = spans.map(span => ({ key: span.key, tally: tally(applications.filter(application => application.applicationDate !== null && application.applicationDate >= span.start && application.applicationDate < span.end)) }));
  const undated = applications.filter(application => application.applicationDate === null).length;
  const inside = periods.reduce((sum, period) => sum + period.tally.n, 0);
  return { periods, undated, outside: applications.length - undated - inside };
}

/**
 * Text for the cross-job overview: 「給与 2件（選択1件・推定1件）・勤務地 2件（推定2件）／n=7」 (a tie cut by
 * the limit of 2 is named: 「・ほかに同じ件数の分類1つ」), 「記録なし」, or
 * 「未取得」. 選択 and 推定 are always written apart. A stored file without applicant keys counts
 * texts (「記述n=」), one without the category sources says the choices were not read, and a cut
 * list of texts says the counts are partial.
 */
export function overviewReasonText(collection: ApplicantReasonCollection | undefined): string {
  const classified = classifyApplicationReasons(collection);
  if (!collection || !classified) return '未取得';
  const result = tally(classified.applications);
  const notes = [
    ...(collection.selections === null ? ['分類の選択は未取得'] : []),
    ...(collection.truncated ? ['記述の一部だけで集計'] : []),
    ...(classified.multiListing ? [`ほかの求人にも関連する応募${String(classified.multiListing)}件を除く`] : []),
  ];
  const tail = notes.length ? `（${notes.join('・')}）` : '';
  if (!result.n) return collection.selections === null ? `文の記録なし${tail}` : `記録なし${tail}`;
  const n = classified.unit === 'text' ? `記述n=${String(result.n)}（応募ごとではない）` : `n=${String(result.n)}`;
  const { rows: top, tiedOut } = topReasonsWithTies(result);
  if (!top.length) return `分類できる記録なし／${n}${tail}`;
  const tie = tiedOut ? `・ほかに同じ件数の分類${String(tiedOut)}つ` : '';
  return `${top.map(row => categoryCountText(row)).join('・')}${tie}／${n}${tail}`;
}
