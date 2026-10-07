/**
 * 求人文面管理の表示用の書式と言い換え。
 *
 * 画面に出す日時・金額・コード値・並び順・注意書きはここに揃える。
 * 値が無いときに 0 や「なし」に置き換えない（呼び出し側が空表示を選べるよう null / '' を返す）。
 */

const JST_OFFSET_MS = 9 * 60 * 60 * 1000;
const DATE_ONLY = /^(\d{4})-(\d{2})-(\d{2})$/;
const MONTH_ONLY = /^(\d{4})-(\d{2})$/;

const pad = (value: number) => String(value).padStart(2, '0');

/** ISO 文字列を Date にする。マイクロ秒など 3 桁を超える小数秒も受け付ける。読めなければ null。 */
function parseInstant(value: string): Date | null {
  const normalized = value.trim().replace(/(\.\d{3})\d+/, '$1');
  const time = Date.parse(normalized);
  return Number.isNaN(time) ? null : new Date(time);
}

function jstParts(value: string): { year: number; month: number; day: number; hour: number; minute: number } | null {
  const instant = parseInstant(value);
  if (!instant) return null;
  const shifted = new Date(instant.getTime() + JST_OFFSET_MS);
  return { year: shifted.getUTCFullYear(), month: shifted.getUTCMonth() + 1, day: shifted.getUTCDate(), hour: shifted.getUTCHours(), minute: shifted.getUTCMinutes() };
}

/**
 * 日時を `YYYY/MM/DD HH:mm JST` にする。
 * 日付だけ（YYYY-MM-DD）の値は時刻を足さずに `YYYY/MM/DD` にする。読めない値は `fallback`（既定は空文字）。
 */
export function formatDateTimeJst(value: string | null | undefined, fallback = ''): string {
  if (!value) return fallback;
  const dateOnly = DATE_ONLY.exec(value.trim());
  if (dateOnly) return `${dateOnly[1] ?? ''}/${dateOnly[2] ?? ''}/${dateOnly[3] ?? ''}`;
  const parts = jstParts(value);
  if (!parts) return fallback;
  return `${String(parts.year)}/${pad(parts.month)}/${pad(parts.day)} ${pad(parts.hour)}:${pad(parts.minute)} JST`;
}

/** 日付を JST の `YYYY/MM/DD` にする。月だけ（YYYY-MM）の値は `YYYY/MM`。読めない値は `fallback`。 */
export function formatDateJst(value: string | null | undefined, fallback = ''): string {
  if (!value) return fallback;
  const trimmed = value.trim();
  const dateOnly = DATE_ONLY.exec(trimmed);
  if (dateOnly) return `${dateOnly[1] ?? ''}/${dateOnly[2] ?? ''}/${dateOnly[3] ?? ''}`;
  const monthOnly = MONTH_ONLY.exec(trimmed);
  if (monthOnly) return `${monthOnly[1] ?? ''}/${monthOnly[2] ?? ''}`;
  const parts = jstParts(trimmed);
  return parts ? `${String(parts.year)}/${pad(parts.month)}/${pad(parts.day)}` : fallback;
}

/** 期間を `YYYY/MM/DD〜YYYY/MM/DD` にする。片方が無ければ「開始不明」「継続中」で埋める。 */
export function formatPeriodJst(start: string | null | undefined, end: string | null | undefined): string {
  return `${formatDateJst(start, '開始不明')}〜${formatDateJst(end, '継続中')}`;
}

/**
 * 円の金額を日本語の書き方にする。
 * - 4000000 → 400万円、250000 → 25万円、253400 → 25万3,400円
 * - 1100 → 1,100円、120000000 → 1億2,000万円
 * 小数は円未満を四捨五入する。null・NaN は `fallback`（既定は空文字）。0 は「0円」。
 */
export function formatYen(value: number | null | undefined, fallback = ''): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return fallback;
  const rounded = Math.round(Math.abs(value));
  const sign = value < 0 && rounded > 0 ? '−' : '';
  const oku = Math.floor(rounded / 100_000_000);
  const man = Math.floor((rounded % 100_000_000) / 10_000);
  const yen = rounded % 10_000;
  if (!oku && !man) return `${sign}${yen.toLocaleString('ja-JP')}円`;
  const head = `${oku ? `${oku.toLocaleString('ja-JP')}億` : ''}${man ? `${man.toLocaleString('ja-JP')}万` : ''}`;
  return `${sign}${head}${yen ? yen.toLocaleString('ja-JP') : ''}円`;
}

/** 件数・指標値を 3 桁区切りにする（小数は 2 桁まで）。null は `fallback`。 */
export function formatCount(value: number | null | undefined, unit = '', fallback = ''): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return fallback;
  return `${value.toLocaleString('ja-JP', { maximumFractionDigits: 2 })}${unit}`;
}

/** 空の値を除いてつなぐ。値が空の欄の区切り記号（「 · 」など）を出さないため。 */
export function joinPresent(parts: (string | number | null | undefined | false)[], separator = ' · '): string {
  return parts.filter((part): part is string | number => part !== null && part !== undefined && part !== false && String(part).trim() !== '').map(String).join(separator);
}

/** 媒体 CSV の条件欄で使われるコード値の項目。 */
export type ConditionCodeField = 'trial' | 'training' | 'employment' | 'flag';

const yesNo: Record<string, string> = { '1': 'あり', '0': 'なし', true: 'あり', false: 'なし', TRUE: 'あり', FALSE: 'なし', yes: 'あり', no: 'なし', 有: 'あり', 無: 'なし', あり: 'あり', なし: 'なし' };

/** コード → 画面に出すラベル。項目名は `conditionFieldLabels` を使う。 */
export const conditionCodeLabels: Record<ConditionCodeField, Record<string, string>> = {
  trial: yesNo,
  training: yesNo,
  flag: yesNo,
  employment: {
    full_time: '正社員', regular: '正社員', '正社員': '正社員',
    contract: '契約社員', '契約社員': '契約社員',
    part_time: 'パート・アルバイト', part: 'パート・アルバイト', 'パート': 'パート・アルバイト', 'アルバイト': 'パート・アルバイト',
    temporary: '派遣社員', dispatch: '派遣社員', '派遣': '派遣社員',
    outsourcing: '業務委託', '業務委託': '業務委託',
  },
};

export const conditionFieldLabels: Record<ConditionCodeField, string> = { trial: '試用期間', training: '研修', employment: '雇用形態', flag: '' };

/**
 * コード値をラベルにする。登録されていないコードは null を返す（コードをそのまま画面に出さない）。
 * 呼び出し側は null のとき「不明」などを出し、元のコードは title（ツールチップ）に回す。
 */
export function codeLabel(field: ConditionCodeField, code: string | number | boolean | null | undefined): string | null {
  if (code === null || code === undefined) return null;
  const key = String(code).trim();
  if (!key) return null;
  return conditionCodeLabels[field][key] ?? conditionCodeLabels[field][key.toLowerCase()] ?? null;
}

/** 本文の 1 行「試用期間：1」のような条件行のコード値をラベルに置き換える。対象外の行はそのまま返す。 */
export function labelConditionLine(line: string): string {
  const match = /^(\s*)(試用期間|研修(?:制度|期間)?|雇用形態)(\s*[：:]\s*)(.+?)\s*$/.exec(line);
  if (!match) return line;
  const [, indent = '', name = '', separator = '', value = ''] = match;
  const field: ConditionCodeField = name === '雇用形態' ? 'employment' : name === '試用期間' ? 'trial' : 'training';
  const label = codeLabel(field, value);
  return label ? `${indent}${name}${separator}${label}` : line;
}

/** 年代の並び順の基準（下限の年齢）。読めない区分は最後。 */
export function ageBandRank(category: string): number {
  const text = category.trim();
  const below = /^(\d+)歳(?:未満|以下)$/.exec(text);
  if (below) return -1;
  const decade = /^(\d+)代/.exec(text);
  if (decade) return Number(decade[1]);
  const lower = /^(\d+)(?:歳)?(?:以上|〜|~|-|～)/.exec(text);
  if (lower) return Number(lower[1]);
  return Number.POSITIVE_INFINITY;
}

/** 年代を年齢の順に並べる（「20歳未満」は「20代」より前、「不明」は最後）。元の配列は変えない。 */
export function orderAgeBands<T>(items: readonly T[], category: (item: T) => string = item => String(item)): T[] {
  return items.map((item, index) => ({ item, index, rank: ageBandRank(category(item)) }))
    .sort((left, right) => left.rank - right.rank || left.index - right.index)
    .map(entry => entry.item);
}

/** 都道府県（北から南、JIS コード順）。 */
export const PREFECTURES_NORTH_TO_SOUTH = [
  '北海道', '青森県', '岩手県', '宮城県', '秋田県', '山形県', '福島県',
  '茨城県', '栃木県', '群馬県', '埼玉県', '千葉県', '東京都', '神奈川県',
  '新潟県', '富山県', '石川県', '福井県', '山梨県', '長野県', '岐阜県', '静岡県', '愛知県',
  '三重県', '滋賀県', '京都府', '大阪府', '兵庫県', '奈良県', '和歌山県',
  '鳥取県', '島根県', '岡山県', '広島県', '山口県',
  '徳島県', '香川県', '愛媛県', '高知県',
  '福岡県', '佐賀県', '長崎県', '熊本県', '大分県', '宮崎県', '鹿児島県', '沖縄県',
] as const;

/** 都道府県の並び順（北が小さい）。「大分県 / 大分市」「大分県大分市」のように後ろに続く値も先頭の都道府県で決める。不明は最後。 */
export function prefectureRank(category: string): number {
  const text = category.trim();
  const index = PREFECTURES_NORTH_TO_SOUTH.findIndex(name => text.startsWith(name));
  return index === -1 ? Number.POSITIVE_INFINITY : index;
}

/** 都道府県を北から南の順に並べる。同じ都道府県の中と、都道府県が分からない値は元の順を保つ。 */
export function orderPrefectures<T>(items: readonly T[], category: (item: T) => string = item => String(item)): T[] {
  return items.map((item, index) => ({ item, index, rank: prefectureRank(category(item)) }))
    .sort((left, right) => left.rank - right.rank || left.index - right.index)
    .map(entry => entry.item);
}

/** 応募者構成の区分を、その属性に合った順に並べる（年代は年齢順、都道府県・市区町村は北から南）。 */
export function orderCategories<T extends { category: string }>(dimension: string, items: readonly T[]): T[] {
  if (dimension === 'age') return orderAgeBands(items, item => item.category);
  if (dimension === 'prefecture' || dimension === 'municipality') return orderPrefectures(items, item => item.category);
  return [...items];
}

/**
 * サーバーや保存データから届く説明文に残る開発用の言葉を、画面向けの言葉に置き換える。
 * 長い語から順に置き換える（「観測版」を「観測」より先に）。
 */
const plainReplacements: [RegExp, string][] = [
  [/source filename acquisition label; not publication timestamp/g, 'ファイルを取得した日時（掲載が変わった日時ではありません）'],
  [/ファイル取得日時による観測ラベル/g, 'ファイルを取得した日時'],
  [/ctk_count/g, 'Indeed閲覧者指標'],
  [/市場閲覧者指標（ctk）/g, 'Indeed閲覧者指標'],
  [/（ctk）|\(ctk\)/g, ''],
  [/\bctk\b/g, 'Indeed閲覧者指標'],
  [/版の対応不明/g, 'どの版への応募か不明'],
  [/版対応不明/g, 'どの版への応募か不明'],
  [/版対応/g, 'どの版への応募か'],
  [/観測ラベル/g, '取得日時'],
  [/本文観測版|掲載観測版|観測版/g, '取得した版'],
  [/本文観測/g, '本文の取得'],
  [/観測日時/g, '取得日時'],
  [/観測日/g, '取得日'],
  [/観測対応/g, '取得日との突き合わせ'],
  [/観測/g, '取得'],
  [/複合集計|複合条件/g, '組み合わせ条件'],
  [/実データMOC/g, '実データ'],
  [/このMOC/g, 'この試作画面'],
  [/（MOC）|\(MOC\)/g, ''],
  [/MOC/g, '試作画面'],
  [/スナップショット|snapshot/gi, '保存データ'],
  [/fixture/gi, 'テスト用データ'],
];

export function plainWording(text: string | null | undefined): string {
  if (!text) return '';
  return plainReplacements.reduce((current, [pattern, replacement]) => current.replace(pattern, replacement), text);
}

/** 画面に出してはいけない開発用の言葉。テストで画面文言を検査するときに使う。 */
export const JARGON_PATTERN = /観測|版対応|ctk|MOC|複合集計|snapshot|スナップショット|fixture|schemaVersion|JSON|\d+\s?ms\b/i;

/** 因果を言い切る言葉。テストで画面文言を検査するときに使う。 */
export const CAUSAL_PATTERN = /効果|確実に|必ず|100%/;

/** 「ⓘ 集計の前提」に必ず入れる 2 つの注意書き。 */
export const HUBSPOT_ONLY_NOTE = '応募はHubSpotに記録されたものだけを数えています。媒体上のすべての応募ではありません。';
export const NOT_CAUSAL_NOTE = '数字は同じ時期に並べて見るためのものです。変更が応募の増減の原因かどうかは、この数字だけでは分かりません。';
