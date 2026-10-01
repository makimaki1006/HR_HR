/**
 * Same digit rules as the old template's fmt():
 * Number#toLocaleString('ja-JP') with min = max fraction digits; null / NaN -> "—".
 */
export function fmt(n: number | null | undefined, digits = 0): string {
  if (n === null || n === undefined || Number.isNaN(n)) return '—';
  return n.toLocaleString('ja-JP', { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

/** Yen -> man-yen (rounded). Null when the value is missing or not positive (old toManYen). */
export function toManYen(v: number | null | undefined): number | null {
  return v !== null && v !== undefined && v > 0 ? Math.round(v / 10000) : null;
}

/** "+" for >= 0 (old commuter inflow / gap cells), "" otherwise (the minus sign comes from fmt). */
export function signPrefix(v: number): string {
  return v >= 0 ? '+' : '';
}
