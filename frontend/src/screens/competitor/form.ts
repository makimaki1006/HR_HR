// 入力フォームの値・検証・送信用 FormData。サーバ側の検証 (competitor.rs) が正で、ここは送る前の親切。
// フィールド名は旧画面 (templates/competitor.html) と同じ。

/** サーバの上限 (lib.rs の DefaultBodyLimit) と同じ。 */
export const MAX_CSV_BYTES = 20 * 1024 * 1024;
export const TEXT_MAX_CHARS = 200;
export const TOP_N_MIN = 1;
export const TOP_N_MAX = 200;

export const PREFECTURE_NOTICE =
  '全国のままだと「人口・地域データ」タブは表示されません。人口・地域データを見る場合は都道府県を選んでください。';

export type SourceType = 'indeed_sp' | 'indeed';
export type WageMode = 'monthly' | 'hourly';

export interface FormValues {
  surveyTitle: string;
  marketTitle: string;
  prefecture: string;
  searchKeyword: string;
  includeGoogle: boolean;
  sourceType: SourceType;
  wageMode: WageMode;
  topN: string;
  file: File | null;
}

export const DEFAULT_FORM: FormValues = {
  surveyTitle: '',
  marketTitle: '',
  prefecture: '',
  searchKeyword: '',
  includeGoogle: true,
  sourceType: 'indeed_sp',
  wageMode: 'monthly',
  topN: '45',
  file: null,
};

export type FormErrors = Partial<
  Record<'file' | 'topN' | 'surveyTitle' | 'searchKeyword' | 'prefecture', string>
>;

/** CSV だけの検証 (選択した時点で指摘するのにも使う)。 */
export function validateFile(file: File | null): string | undefined {
  if (file === null) return '求人一覧CSVを選択してください。';
  if (file.size === 0) return 'CSVファイルが空です。内容のあるファイルを選んでください。';
  if (file.size > MAX_CSV_BYTES) return 'CSVのサイズが上限(20MB)を超えています。';
  if (!/\.(csv|txt)$/i.test(file.name)) {
    return 'ファイルの拡張子は .csv または .txt にしてください。Excelブックは求人一覧シートをCSVに書き出してください。';
  }
  return undefined;
}

/** 空白を除いて 1〜200 の整数だけ。小数・指数表記・符号付きは不可。 */
export function parseTopN(raw: string): number | null {
  const s = raw.trim();
  if (!/^\d+$/.test(s)) return null;
  const n = Number(s);
  return n >= TOP_N_MIN && n <= TOP_N_MAX ? n : null;
}

/** `prefectures` が空 (選択肢をまだ取得できていない) ときは都道府県を検証しない。 */
export function validateForm(values: FormValues, prefectures: readonly string[]): FormErrors {
  const errors: FormErrors = {};
  const fileError = validateFile(values.file);
  if (fileError !== undefined) errors.file = fileError;
  if (parseTopN(values.topN) === null) {
    errors.topN = `比較する件数は ${String(TOP_N_MIN)}〜${String(TOP_N_MAX)} の整数で入力してください。`;
  }
  if (Array.from(values.surveyTitle).length > TEXT_MAX_CHARS) {
    errors.surveyTitle = `調査名は ${String(TEXT_MAX_CHARS)} 文字以内にしてください。`;
  }
  if (Array.from(values.searchKeyword).length > TEXT_MAX_CHARS) {
    errors.searchKeyword = `検索語は ${String(TEXT_MAX_CHARS)} 文字以内にしてください。`;
  }
  if (
    values.prefecture !== '' &&
    prefectures.length > 0 &&
    !prefectures.includes(values.prefecture)
  ) {
    errors.prefecture = '都道府県を選択肢から選んでください。';
  }
  return errors;
}

/** 旧画面と同じ name で組む。`include_google` はチェックボックスと同じで、OFF のときは送らない。 */
export function buildFormData(values: FormValues): FormData {
  const data = new FormData();
  data.set('survey_title', values.surveyTitle);
  data.set('market_title', values.marketTitle);
  data.set('prefecture', values.prefecture);
  data.set('search_keyword', values.searchKeyword);
  if (values.includeGoogle) data.set('include_google', '1');
  data.set('source_type', values.sourceType);
  data.set('wage_mode', values.wageMode);
  data.set('top_n', values.topN.trim());
  if (values.file) data.set('csv_file', values.file, values.file.name);
  return data;
}
