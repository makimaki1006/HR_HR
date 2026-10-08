/**
 * Pull the pay condition out of a job body ("給与：月給250,000円〜280,000円").
 * There is no structured salary field, so this reads the labelled line only. When the line
 * exists but cannot be read, the kind is '不明' (never guessed from surrounding text or from the
 * size of the amount, and never only one end of a range).
 */
export type SalaryKind = '月給' | '時給' | '日給' | '年収' | '不明';
export interface SalaryInfo {
  kind: SalaryKind;
  /** Yen. null when the amount could not be read. */
  min: number | null;
  max: number | null;
  /** The text after the label, as written in the body. */
  raw: string;
}

const LABEL = /^\s*[【[（(]?\s*(給与|給料|賃金|報酬|月給|時給|日給|年収|年俸)\s*[】\]）)]?\s*(?:[：:]\s*|\s+|$|(?=\d))(.*)$/;
const KINDS: Record<string, SalaryKind> = { 月給: '月給', 時給: '時給', 日給: '日給', 年収: '年収', 年俸: '年収' };
const KIND_WORD = /(月給|時給|日給|年収|年俸)/g;
// "25万円", "1万2000円", "1,100円", "18" (a bare number). Group 3 is the digits right after 万.
const AMOUNT = /(\d+(?:\.\d+)?)(?:\s*(万)(\d{1,4}(?![\d.]))?)?\s*(円)?/g;
const RANGE_SEPARATOR = /^\s*[〜~\-–ー－]\s*$/;
// The yen range a pay kind must fall in to be believable (時給 of 10万円 is a misread, not a fact).
const PLAUSIBLE: Partial<Record<SalaryKind, [number, number]>> = {
  時給: [100, 99_999], 日給: [1_000, 499_999], 月給: [10_000, 9_999_999], 年収: [100_000, 999_999_999],
};

interface Token {
  value: number | null; unit: 'man' | 'yen' | 'none'; start: number; end: number;
  /** 円 is written right after the amount. */
  yen: boolean;
  /**
   * "1万2000" with no 円 after it: 12,000 when it is the bottom of a range whose top ends in 円
   * ("日給1万2000〜1万5000円"); unreadable otherwise.
   */
  bottomOnly?: number;
}

function tokens(text: string): Token[] {
  const found: Token[] = [];
  for (const match of text.matchAll(AMOUNT)) {
    const [whole, number, man, after, en] = match;
    if (number === undefined) continue;
    const base = Number(number);
    let value: number | null = Number.isFinite(base) ? base : null;
    let bottomOnly: number | undefined;
    if (value !== null && man) {
      // "1万2000円" is 12,000円. Digits after 万 without 円 cannot be read on their own.
      if (after !== undefined) {
        const read = Math.round(value * 10_000 + Number(after));
        value = en ? read : null;
        if (!en) bottomOnly = read;
      } else value = Math.round(value * 10_000);
    }
    found.push({ value, unit: man ? 'man' : en ? 'yen' : 'none', start: match.index, end: match.index + whole.length, yen: Boolean(en), ...(bottomOnly === undefined ? {} : { bottomOnly }) });
  }
  return found;
}

const unreadable = (raw: string): SalaryInfo => ({ kind: '不明', min: null, max: null, raw });

/** Reads one salary text such as "月給25万円〜28万円", "月給18〜25万円" or "日給1万2000円". */
export function parseSalaryText(text: string): SalaryInfo {
  const raw = text.trim();
  // 日給月給 is a form of monthly pay.
  const normalized = raw.normalize('NFKC').replace(/(\d),(?=\d{3})/g, '$1').replace(/日給月給/g, '月給');
  const all = tokens(normalized);
  const separated = (left: Token, right: Token | undefined) => Boolean(right && RANGE_SEPARATOR.test(normalized.slice(left.end, right.start)));
  // The first pay amount. Hours and days ("8時間") are small bare numbers and are skipped, except a
  // bare number right before "〜<amount with 万 or 円>", which is the bottom of a range ("18〜25万円").
  const index = all.findIndex((token, at) => {
    if (token.unit !== 'none' || (token.value !== null && token.value >= 500)) return true;
    const next = all[at + 1];
    return Boolean(next && next.unit !== 'none' && separated(token, next));
  });
  const first = all[index];
  if (!first) return unreadable(raw);
  // The kind word nearest before the first amount ("25万円 ※時給換算1,500円" has none before it).
  const kindWord = [...normalized.slice(0, first.start).matchAll(KIND_WORD)].at(-1)?.[1];
  const kind = kindWord ? KINDS[kindWord] ?? '不明' : '不明';
  if (kind === '不明') return unreadable(raw);
  const second = all[index + 1];
  const adjacent = second !== undefined && separated(first, second);
  let min = first.value;
  // "1万2000〜1万5000円": the bottom has no 円, but the top it is joined to ends in 円.
  if (min === null && first.bottomOnly !== undefined && adjacent && second.value !== null && second.yen) min = first.bottomOnly;
  // The bottom of "18〜25万円" takes the unit written after the top.
  if (adjacent && first.unit === 'none' && min !== null && second.unit === 'man') min = Math.round(min * 10_000);
  if (min === null) return unreadable(raw);
  // A range is only "<amount> 〜 <amount>" written next to each other. A later amount (an allowance,
  // a training wage) is not the top of the range. A range that cannot be read is 不明, never one end.
  let max = min;
  if (adjacent) {
    if (second.value === null) return unreadable(raw);
    if (second.value >= min) max = second.value;
    else if (first.unit === 'none') return unreadable(raw);
  }
  const bounds = PLAUSIBLE[kind];
  if (bounds && (min < bounds[0] || max > bounds[1])) return unreadable(raw);
  return { kind, min, max, raw };
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
    if (!rest) return unreadable('');
    return parseSalaryText(KINDS[label] && !/(月給|時給|日給|年収|年俸)/.test(rest) ? `${label}${rest}` : rest);
  }
  return null;
}

export function sameSalary(left: SalaryInfo | null, right: SalaryInfo | null): boolean {
  if (!left || !right) return left === right;
  return left.kind === right.kind && left.min === right.min && left.max === right.max;
}

const man = (value: number) => `${(value / 10_000).toLocaleString('ja-JP', { maximumFractionDigits: 1 })}万`;
/** Short label for the timeline ("月給25万〜28万円", "時給1,100円", "日給12,000円"). */
export function salaryLabel(info: SalaryInfo | null): string {
  if (!info) return '給与の記載なし';
  if (info.kind === '不明' || info.min === null) return '不明';
  const format = (value: number) => info.kind === '時給' || info.kind === '日給' ? value.toLocaleString('ja-JP') : man(value);
  const range = info.max !== null && info.max !== info.min ? `${format(info.min)}〜${format(info.max)}` : format(info.min);
  return `${info.kind}${range}円`;
}

/** True when the line is the salary line (used to tell salary edits from other body edits). */
export function isSalaryLine(line: string): boolean {
  return LABEL.test(line.normalize('NFKC'));
}
