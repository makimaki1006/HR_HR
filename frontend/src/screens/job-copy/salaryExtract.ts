/**
 * Pull the pay condition out of a job body ("給与：月給250,000円〜280,000円").
 * There is no structured salary field, so this reads the labelled line only. When the line
 * exists but cannot be read, the kind is '不明' (never guessed from surrounding text).
 */
export type SalaryKind = '月給' | '時給' | '日給' | '年収' | '不明';
export interface SalaryInfo {
  kind: SalaryKind;
  /** Yen. null when the amount could not be read. */
  min: number | null;
  max: number | null;
  /** The text after the label, as written in the body. */
  raw: string;
  /** True when the kind was not written and was read from the amount alone (e.g. "4000000"). */
  inferredKind: boolean;
}

const LABEL = /^\s*[【[（(]?\s*(給与|給料|賃金|報酬|月給|時給|日給|年収|年俸)\s*[】\]）)]?\s*(?:[：:]\s*|\s+|$|(?=\d))(.*)$/;
const KINDS: Record<string, SalaryKind> = { 月給: '月給', 時給: '時給', 日給: '日給', 年収: '年収', 年俸: '年収' };
const AMOUNT = /(\d+(?:\.\d+)?)\s*(万)?\s*円?/g;

function yen(number: string, man: string | undefined): number | null {
  const value = Number(number);
  if (!Number.isFinite(value)) return null;
  return Math.round(man ? value * 10_000 : value);
}

/** Reads one salary text such as "月給25万円〜28万円" or "時給1,100円". */
export function parseSalaryText(text: string): SalaryInfo {
  const raw = text.trim();
  const normalized = raw.normalize('NFKC').replace(/(\d),(?=\d{3})/g, '$1');
  const kindWord = /(月給|時給|日給|年収|年俸)/.exec(normalized)?.[1];
  // Valid pay amounts with where they sit in the text.
  const amounts: { value: number; start: number; end: number }[] = [];
  for (const match of normalized.matchAll(AMOUNT)) {
    const number = match[1];
    if (number === undefined) continue;
    const value = yen(number, match[2]);
    // Skip hours, days and similar small numbers that are not pay amounts.
    if (value !== null && (match[2] || match[0].includes('円') || value >= 500)) amounts.push({ value, start: match.index, end: match.index + match[0].length });
    if (amounts.length === 2) break;
  }
  const first = amounts[0]; const second = amounts[1];
  const min = first?.value ?? null;
  // A range is only "<amount> 〜 <amount>" written next to each other. A later amount (an allowance,
  // a training wage) is not the top of the range, and a top lower than the bottom is never accepted.
  const adjacent = first && second && /^\s*[〜~\-–ー－]\s*$/.test(normalized.slice(first.end, second.start));
  const max = adjacent && second.value >= first.value ? second.value : min;
  if (kindWord) return { kind: KINDS[kindWord] ?? '不明', min, max, raw, inferredKind: false };
  // Kind not written: only a 7-digit-or-more yen amount is read as annual pay (marked as inferred).
  if (min !== null && min >= 1_000_000) return { kind: '年収', min, max, raw, inferredKind: true };
  return { kind: '不明', min: null, max: null, raw, inferredKind: false };
}

/** null when the body has no salary line. */
export function extractSalary(body: string | null | undefined): SalaryInfo | null {
  if (!body) return null;
  const lines = body.split(/\r?\n/);
  for (let index = 0; index < lines.length; index++) {
    const line = lines[index] ?? '';
    const match = LABEL.exec(line.normalize('NFKC'));
    if (!match) continue;
    const label = match[1] ?? '';
    let rest = (match[2] ?? '').trim();
    if (!rest) {
      // Label on its own line ("【給与】" then the amount on the next line).
      rest = lines.slice(index + 1).find(next => next.trim() !== '')?.trim() ?? '';
    }
    if (!rest) return { kind: '不明', min: null, max: null, raw: '', inferredKind: false };
    return parseSalaryText(KINDS[label] && !/(月給|時給|日給|年収|年俸)/.test(rest) ? `${label}${rest}` : rest);
  }
  return null;
}

export function sameSalary(left: SalaryInfo | null, right: SalaryInfo | null): boolean {
  if (!left || !right) return left === right;
  return left.kind === right.kind && left.min === right.min && left.max === right.max;
}

const man = (value: number) => `${(value / 10_000).toLocaleString('ja-JP', { maximumFractionDigits: 1 })}万`;
/** Short label for the timeline ("月給25万〜28万円", "時給1,100円", "年収400万円（推定表記）"). */
export function salaryLabel(info: SalaryInfo | null): string {
  if (!info) return '給与の記載なし';
  if (info.kind === '不明' || info.min === null) return '不明';
  const format = (value: number) => info.kind === '時給' || info.kind === '日給' ? value.toLocaleString('ja-JP') : man(value);
  const range = info.max !== null && info.max !== info.min ? `${format(info.min)}〜${format(info.max)}` : format(info.min);
  return `${info.kind}${range}円${info.inferredKind ? '（推定表記）' : ''}`;
}

/** True when the line is the salary line (used to tell salary edits from other body edits). */
export function isSalaryLine(line: string): boolean {
  return LABEL.test(line.normalize('NFKC'));
}
