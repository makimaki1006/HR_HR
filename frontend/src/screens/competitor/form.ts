// 入力フォームの値・検証・送信用 FormData。サーバ側の検証 (competitor.rs) が正で、ここは送る前の親切。
// フィールド名は旧画面 (templates/competitor.html) と同じ。

/** サーバの上限 (lib.rs の DefaultBodyLimit) と同じ。 */
export const MAX_CSV_BYTES = 20 * 1024 * 1024;
export const TEXT_MAX_CHARS = 200;
/** 職種名の上限 (サーバと同じ 40 文字)。 */
export const JOB_NAME_MAX_CHARS = 40;
export const TOP_N_MIN = 1;
export const TOP_N_MAX = 200;

export type SourceType = 'indeed_sp' | 'indeed';
export type WageMode = 'monthly' | 'hourly';

export interface FormValues {
  jobName: string;
  marketTitle: string;
  prefecture: string;
  municipality: string;
  searchKeyword: string;
  includeGoogle: boolean;
  sourceType: SourceType;
  wageMode: WageMode;
  topN: string;
  file: File | null;
}

export const DEFAULT_FORM: FormValues = {
  jobName: '',
  marketTitle: '',
  prefecture: '',
  municipality: '',
  searchKeyword: '',
  includeGoogle: true,
  sourceType: 'indeed_sp',
  wageMode: 'monthly',
  topN: '45',
  file: null,
};

export type FormErrors = Partial<
  Record<'file' | 'topN' | 'jobName' | 'searchKeyword' | 'prefecture' | 'municipality', string>
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

/**
 * `prefectures` が空 (選択肢をまだ取得できていない) ときは都道府県・市区町村を検証しない。
 * `municipalities` は都道府県ごとの市区町村の選択肢。
 */
export function validateForm(
  values: FormValues,
  prefectures: readonly string[],
  municipalities: Readonly<Record<string, readonly string[] | undefined>> = {},
): FormErrors {
  const errors: FormErrors = {};
  const fileError = validateFile(values.file);
  if (fileError !== undefined) errors.file = fileError;
  if (parseTopN(values.topN) === null) {
    errors.topN = `比較する件数は ${String(TOP_N_MIN)}〜${String(TOP_N_MAX)} の整数で入力してください。`;
  }
  if (Array.from(values.jobName.trim()).length > JOB_NAME_MAX_CHARS) {
    errors.jobName = `職種名は ${String(JOB_NAME_MAX_CHARS)} 文字以内にしてください。`;
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
  if (values.municipality !== '' && prefectures.length > 0) {
    const cities = municipalities[values.prefecture];
    if (values.prefecture === '' || !cities?.includes(values.municipality)) {
      errors.municipality = '市区町村を選択肢から選んでください。';
    }
  }
  return errors;
}

/** 旧画面と同じ name で組む。`include_google` はチェックボックスと同じで、OFF のときは送らない。 */
export function buildFormData(values: FormValues, progressId?: string): FormData {
  const data = new FormData();
  if (progressId !== undefined) data.set('progress_id', progressId);
  data.set('market_title', values.marketTitle);
  data.set('prefecture', values.prefecture);
  data.set('municipality', values.municipality);
  data.set('job_name', values.jobName.trim());
  data.set('search_keyword', values.searchKeyword);
  if (values.includeGoogle) data.set('include_google', '1');
  data.set('source_type', values.sourceType);
  data.set('wage_mode', values.wageMode);
  data.set('top_n', values.topN.trim());
  if (values.file) data.set('csv_file', values.file, values.file.name);
  return data;
}
