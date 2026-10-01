/**
 * Same digit rules as the old template's fmt():
 * Number#toLocaleString('ja-JP') with min = max fraction digits; null / NaN -> "—".
 * Two differences from the old page, both on purpose: Infinity is a dash too (the Rust side
 * writes it as null anyway), and a negative zero ("-0", "-0.00" from -0 or a tiny negative that
 * rounds to zero) prints without the minus sign.
 */
export function fmt(n: number | null | undefined, digits = 0): string {
  if (n === null || n === undefined || !Number.isFinite(n)) return '—';
  const s = n.toLocaleString('ja-JP', { minimumFractionDigits: digits, maximumFractionDigits: digits });
  return /^[-−]0(\.0+)?$/.test(s) ? s.slice(1) : s;
}

/** v * k, or null when v is missing (null * 100 would be a plausible-looking 0). */
export function scale(v: number | null | undefined, k: number): number | null {
  return v === null || v === undefined || !Number.isFinite(v) ? null : v * k;
}

/** Yen -> man-yen (rounded). Null when the value is missing or not positive (old toManYen). */
export function toManYen(v: number | null | undefined): number | null {
  return v !== null && v !== undefined && v > 0 ? Math.round(v / 10000) : null;
}

/** "+" for >= 0 (old commuter inflow / gap cells), "" otherwise (the minus sign comes from fmt). */
export function signPrefix(v: number | null | undefined): string {
  return v !== null && v !== undefined && Number.isFinite(v) && v >= 0 ? '+' : '';
}
