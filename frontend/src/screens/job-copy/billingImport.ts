import type { JobCopyRecord } from './data';
import type { BillingMedia, BillingPeriod, BillingTaxBasis } from './billingTypes';

/**
 * 課金CSVの解析・検証・求人との突き合わせ。
 *
 * - 文字コードは UTF-8 (BOM の有無を問わない) と CP932 (Excel の日本語 CSV) を受け付ける。
 * - 列の対応は `BillingColumnAliases` で差し替えられる。列名がまだ確定していないため。
 * - 求人との結びつけは「媒体 + 媒体求人ID」の完全一致だけ。タイトルで推測しない。
 * - 同じ求人・同じ期間の行は 2 行目以降を反映しない (合算しない)。
 * - 金額が空欄の行は amountYen = null。0 円にしない。
 *
 * 取り込んだ結果はブラウザのメモリ上だけで持つ。サーバーへ送らず、再読み込みで消える。
 */

export const MAX_BILLING_FILE_BYTES = 5 * 1024 * 1024;
export const MAX_BILLING_ROWS = 20_000;

export type BillingField = 'media' | 'mediaJobId' | 'periodStart' | 'periodEnd' | 'amount' | 'planName' | 'impressions' | 'clicks' | 'mediaApplications';
export type BillingEncoding = 'utf-8' | 'shift_jis';

export interface BillingFieldSpec { field: BillingField; label: string; required: boolean }
export const BILLING_FIELDS: readonly BillingFieldSpec[] = [
  { field: 'media', label: '媒体', required: true },
  { field: 'mediaJobId', label: '媒体求人ID', required: true },
  { field: 'periodStart', label: '期間開始', required: true },
  { field: 'periodEnd', label: '期間終了', required: true },
  { field: 'amount', label: '金額（円）', required: true },
  { field: 'planName', label: 'プラン名', required: false },
  { field: 'impressions', label: '表示回数', required: false },
  { field: 'clicks', label: 'クリック数', required: false },
  { field: 'mediaApplications', label: '媒体の応募数', required: false },
];

/** 項目ごとに、見出しとして受け付ける列名。差し替え可能。比較は全角半角・大文字小文字・空白と括弧書きを無視する。 */
export type BillingColumnAliases = Readonly<Record<BillingField, readonly string[]>>;
export const DEFAULT_BILLING_COLUMN_ALIASES: BillingColumnAliases = {
  media: ['媒体', '媒体名', 'media'],
  mediaJobId: ['媒体求人ID', '求人ID', '媒体ID', 'media_job_id', 'job_id'],
  periodStart: ['期間開始', '開始日', '掲載開始', '課金開始', 'period_start'],
  periodEnd: ['期間終了', '終了日', '掲載終了', '課金終了', 'period_end'],
  amount: ['金額', '請求金額', '課金額', '費用', 'cost_yen', 'amount'],
  planName: ['プラン名', 'プラン', 'plan'],
  impressions: ['表示回数', 'インプレッション', 'impressions'],
  clicks: ['クリック数', 'クリック', 'clicks'],
  mediaApplications: ['媒体応募数', '媒体の応募数', '応募数', 'applications'],
};

/** 項目 → CSV の列番号 (0 始まり)。未対応の項目は undefined。 */
export type BillingColumnMapping = Partial<Record<BillingField, number>>;

export interface BillingRowIssue { row: number; message: string }

export interface BillingImportResult {
  /** 求人に一致し、反映できる課金期間。 */
  periods: BillingPeriod[];
  /** 値の誤りで受け付けなかった行。 */
  rejected: BillingRowIssue[];
  /** 媒体 + 媒体求人ID に一致する求人が一覧に無い行。 */
  notFound: BillingRowIssue[];
  /** 媒体 + 媒体求人ID に一致する求人が 2 件以上ある行。結びつけない。 */
  ambiguous: BillingRowIssue[];
  /** 同じ求人・同じ期間の 2 行目以降。合算せず反映しない。 */
  duplicates: BillingRowIssue[];
  /** 反映はするが注意が要る行 (期間の重なり、金額の空欄)。 */
  warnings: BillingRowIssue[];
  counts: { dataRows: number; matched: number; ambiguous: number; notFound: number; rejected: number; duplicates: number };
}

export class BillingCsvError extends Error {}

// ---------- 文字コード ----------

/** CSV のバイト列を文字列にする。auto は BOM → UTF-8 → CP932 の順に試す。 */
export function decodeBillingCsv(bytes: ArrayBuffer | Uint8Array, encoding: BillingEncoding | 'auto' = 'auto'): { text: string; encoding: BillingEncoding } {
  const view = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  const hasBom = view.length >= 3 && view[0] === 0xef && view[1] === 0xbb && view[2] === 0xbf;
  const order: BillingEncoding[] = encoding === 'auto' ? (hasBom ? ['utf-8'] : ['utf-8', 'shift_jis']) : [encoding];
  for (const candidate of order) {
    try {
      const text = new TextDecoder(candidate, { fatal: true }).decode(view);
      return { text: text.startsWith('﻿') ? text.slice(1) : text, encoding: candidate };
    } catch { /* 次の文字コードを試す */ }
  }
  throw new BillingCsvError(encoding === 'auto'
    ? '文字を読み取れませんでした。UTF-8 か、Excel で保存した日本語の CSV を選んでください。'
    : '選んだ文字コードでは読み取れませんでした。別の文字コードを選んでください。');
}

// ---------- CSV ----------

/** RFC 4180 の CSV を行の配列にする。引用符内の改行・カンマ・"" を扱う。 */
export function parseCsv(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = '';
  let quoted = false;
  let fieldStarted = false;
  for (let index = 0; index < text.length; index++) {
    const char = text.charAt(index);
    if (quoted) {
      if (char === '"') {
        if (text[index + 1] === '"') { field += '"'; index++; } else quoted = false;
      } else field += char;
      continue;
    }
    if (char === '"' && !fieldStarted) { quoted = true; fieldStarted = true; }
    else if (char === ',') { row.push(field); field = ''; fieldStarted = false; }
    else if (char === '\r' || char === '\n') {
      if (char === '\r' && text[index + 1] === '\n') index++;
      row.push(field); rows.push(row); row = []; field = ''; fieldStarted = false;
    } else { field += char; fieldStarted = true; }
  }
  if (quoted) throw new BillingCsvError('引用符 (") が閉じられていない値があります。CSV の形を確認してください。');
  if (fieldStarted || row.length > 0) { row.push(field); rows.push(row); }
  return rows;
}

// ---------- 列の対応 ----------

const normalizeLabel = (value: string) => value.normalize('NFKC').replace(/[(（[【].*$/u, '').replace(/[\s_・]/gu, '').toLowerCase();

/** 見出しから列の対応を推測する。同じ列を 2 項目に割り当てない。 */
export function guessBillingColumns(headers: readonly string[], aliases: BillingColumnAliases = DEFAULT_BILLING_COLUMN_ALIASES): BillingColumnMapping {
  const normalized = headers.map(normalizeLabel);
  const used = new Set<number>();
  const mapping: BillingColumnMapping = {};
  for (const { field } of BILLING_FIELDS) {
    for (const alias of aliases[field]) {
      const target = normalizeLabel(alias);
      const index = normalized.findIndex((header, position) => header === target && !used.has(position));
      if (index >= 0) { mapping[field] = index; used.add(index); break; }
    }
  }
  return mapping;
}

/** 列の対応の不足・重複を返す。空なら取り込める。 */
export function billingMappingProblems(mapping: BillingColumnMapping, columnCount: number): string[] {
  const problems: string[] = [];
  const seen = new Map<number, string>();
  for (const spec of BILLING_FIELDS) {
    const index = mapping[spec.field];
    if (index === undefined) { if (spec.required) problems.push(`「${spec.label}」の列を選んでください。`); continue; }
    if (index < 0 || index >= columnCount) { problems.push(`「${spec.label}」の列がファイルにありません。`); continue; }
    const other = seen.get(index);
    if (other) problems.push(`「${other}」と「${spec.label}」に同じ列が選ばれています。`);
    else seen.set(index, spec.label);
  }
  return problems;
}

/** 金額列の見出しから税込・税抜を読み取る。書かれていなければ不明。 */
export function guessTaxBasis(header: string | undefined): BillingTaxBasis {
  if (!header) return '不明';
  const value = header.normalize('NFKC');
  if (value.includes('税込')) return '税込';
  if (value.includes('税抜') || value.includes('税別')) return '税抜';
  return '不明';
}

// ---------- 値 ----------

const MEDIA_ALIASES: Readonly<Record<string, BillingMedia>> = {
  airワーク: 'Airワーク', airwork: 'Airワーク', エアワーク: 'Airワーク',
  hrハッカー: 'HRハッカー', hrhacker: 'HRハッカー', hrhackerjob: 'HRハッカー', エイチアールハッカー: 'HRハッカー',
};
/** 媒体名を求人一覧の表記にそろえる。知らない媒体は null。 */
export function canonicalMedia(value: string): BillingMedia | null {
  return MEDIA_ALIASES[value.normalize('NFKC').replace(/[\s\-_・]/gu, '').toLowerCase()] ?? null;
}

/** YYYY-MM-DD / YYYY/MM/DD / YYYY/M/D を YYYY-MM-DD にする。暦にない日付は null。 */
export function billingDate(value: string): string | null {
  const match = /^(\d{4})[-/](\d{1,2})[-/](\d{1,2})$/u.exec(value.normalize('NFKC').trim());
  if (!match) return null;
  const [, year = '', month = '', day = ''] = match;
  const iso = `${year}-${month.padStart(2, '0')}-${day.padStart(2, '0')}`;
  const parsed = new Date(`${iso}T00:00:00Z`);
  return Number.isFinite(parsed.getTime()) && parsed.toISOString().slice(0, 10) === iso ? iso : null;
}

type NumberResult = { ok: true; value: number | null } | { ok: false };
/** 空欄は null。カンマ・円記号・「円」を外して数にする。負の数・数でない値は誤り。 */
function amountValue(raw: string, integer: boolean): NumberResult {
  const value = raw.normalize('NFKC').replace(/[,\s¥円]/gu, '');
  if (value === '') return { ok: true, value: null };
  if (!(integer ? /^\d+$/u : /^\d+(?:\.\d+)?$/u).test(value)) return { ok: false };
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed <= Number.MAX_SAFE_INTEGER ? { ok: true, value: parsed } : { ok: false };
}

// ---------- 取り込み ----------

const jobKey = (media: BillingMedia, mediaJobId: string) => `${media}\u0000${mediaJobId}`;

/**
 * CSV の行 (見出し行を含む) と列の対応から、課金期間と行ごとの結果を作る。
 * 行番号は CSV の行 (レコード) の番号で、見出し行が 1。
 */
export function buildBillingImport(rows: readonly (readonly string[])[], mapping: BillingColumnMapping, records: readonly JobCopyRecord[], taxBasis: BillingTaxBasis = '不明'): BillingImportResult {
  const problems = billingMappingProblems(mapping, rows[0]?.length ?? 0);
  if (problems.length) throw new BillingCsvError(problems.join(' '));
  if (rows.length - 1 > MAX_BILLING_ROWS) throw new BillingCsvError(`${String(MAX_BILLING_ROWS)}行までの CSV を選んでください。`);

  const index = new Map<string, JobCopyRecord[]>();
  for (const job of records) {
    const media = canonicalMedia(job.media);
    if (!media) continue;
    const key = jobKey(media, job.mediaJobId.trim());
    index.set(key, [...(index.get(key) ?? []), job]);
  }

  const result: BillingImportResult = { periods: [], rejected: [], notFound: [], ambiguous: [], duplicates: [], warnings: [], counts: { dataRows: 0, matched: 0, ambiguous: 0, notFound: 0, rejected: 0, duplicates: 0 } };
  const firstRowByPeriod = new Map<string, number>();
  const cell = (row: readonly string[], field: BillingField) => {
    const column = mapping[field];
    return column === undefined ? '' : (row[column] ?? '').trim();
  };

  rows.slice(1).forEach((row, offset) => {
    const rowNumber = offset + 2;
    if (row.every(value => value.trim() === '')) return;
    result.counts.dataRows++;
    const errors: string[] = [];
    const mediaRaw = cell(row, 'media');
    const media = canonicalMedia(mediaRaw);
    if (!mediaRaw) errors.push('媒体が空欄です');
    else if (!media) errors.push(`媒体「${mediaRaw}」は扱えません（Airワーク か HRハッカー）`);
    const mediaJobId = cell(row, 'mediaJobId');
    if (!mediaJobId) errors.push('媒体求人IDが空欄です');
    const start = billingDate(cell(row, 'periodStart'));
    const end = billingDate(cell(row, 'periodEnd'));
    if (!start) errors.push('期間開始は 2026-09-01 の形で入れてください');
    if (!end) errors.push('期間終了は 2026-09-30 の形で入れてください');
    if (start && end && end < start) errors.push('期間終了が期間開始より前です');
    const amount = amountValue(cell(row, 'amount'), false);
    if (!amount.ok) errors.push('金額が数として読めません');
    const counts = (['impressions', 'clicks', 'mediaApplications'] as const).map(field => ({ field, parsed: amountValue(cell(row, field), true) }));
    for (const { field, parsed } of counts) {
      if (!parsed.ok) errors.push(`${BILLING_FIELDS.find(spec => spec.field === field)?.label ?? field}が 0 以上の整数として読めません`);
    }
    if (errors.length || !media || !start || !end || !amount.ok) {
      result.rejected.push({ row: rowNumber, message: errors.join('。') });
      return;
    }
    const numbers = Object.fromEntries(counts.map(({ field, parsed }) => [field, parsed.ok ? parsed.value : null])) as Record<'impressions' | 'clicks' | 'mediaApplications', number | null>;

    const periodKey = `${jobKey(media, mediaJobId)}\u0000${start}\u0000${end}`;
    const first = firstRowByPeriod.get(periodKey);
    if (first !== undefined) {
      result.duplicates.push({ row: rowNumber, message: `${String(first)}行目と同じ求人・同じ期間です。合算せず、${String(first)}行目だけを使います` });
      return;
    }
    firstRowByPeriod.set(periodKey, rowNumber);

    const matches = index.get(jobKey(media, mediaJobId)) ?? [];
    if (matches.length === 0) { result.notFound.push({ row: rowNumber, message: `${media} の求人ID「${mediaJobId}」は一覧にありません` }); return; }
    if (matches.length > 1) { result.ambiguous.push({ row: rowNumber, message: `${media} の求人ID「${mediaJobId}」に当てはまる求人が ${String(matches.length)} 件あるため結びつけません` }); return; }
    const [job] = matches;
    if (!job) return;
    if (amount.value === null) result.warnings.push({ row: rowNumber, message: '金額が空欄です。0円ではなく「金額不明」として扱います' });
    const planName = cell(row, 'planName');
    result.periods.push({
      jobId: job.id, media, mediaJobId, periodStart: start, periodEnd: end,
      amountYen: amount.value, taxBasis, planName: planName || null,
      impressions: numbers.impressions, clicks: numbers.clicks, mediaApplications: numbers.mediaApplications,
      source: 'csv', sourceRow: rowNumber, overlapsSourceRows: [],
    });
  });

  // 同じ求人で期間が重なる行。合算はタイムライン側でもしない前提で、行番号を残して知らせる。
  const byJob = new Map<string, BillingPeriod[]>();
  for (const period of result.periods) byJob.set(period.jobId, [...(byJob.get(period.jobId) ?? []), period]);
  for (const periods of byJob.values()) {
    for (const a of periods) {
      for (const b of periods) {
        if (a !== b && b.sourceRow !== null && a.periodStart <= b.periodEnd && b.periodStart <= a.periodEnd) a.overlapsSourceRows.push(b.sourceRow);
      }
    }
  }
  for (const period of result.periods) {
    if (period.overlapsSourceRows.length && period.sourceRow !== null) {
      result.warnings.push({ row: period.sourceRow, message: `${period.overlapsSourceRows.map(String).join('・')}行目と期間が重なっています。金額は合算しません` });
    }
  }
  result.warnings.sort((a, b) => a.row - b.row);

  result.counts.matched = result.periods.length;
  result.counts.ambiguous = result.ambiguous.length;
  result.counts.notFound = result.notFound.length;
  result.counts.rejected = result.rejected.length;
  result.counts.duplicates = result.duplicates.length;
  return result;
}

/** 文字列の CSV を見出しの推測込みで取り込む (テストと一括処理用)。 */
export function importBillingCsv(text: string, records: readonly JobCopyRecord[], aliases: BillingColumnAliases = DEFAULT_BILLING_COLUMN_ALIASES): BillingImportResult {
  const rows = parseCsv(text);
  const headers = rows[0];
  if (!headers) throw new BillingCsvError('CSV が空です。');
  const mapping = guessBillingColumns(headers, aliases);
  const amountColumn = mapping.amount;
  return buildBillingImport(rows, mapping, records, guessTaxBasis(amountColumn === undefined ? undefined : headers[amountColumn]));
}
