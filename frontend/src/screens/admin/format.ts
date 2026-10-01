// Display helpers that mirror the Rust render functions (src/handlers/admin/render.rs,
// src/handlers/my/render.rs) so the React screens show the same text as the old pages.

/** Rust `s.chars().take(n)`: counts Unicode scalar values, not UTF-16 units. */
export function truncateChars(s: string, n: number): string {
  return Array.from(s).slice(0, n).join('');
}

/** Rust: `if a.display_name.is_empty() { "-" }`. */
export function dashIfEmpty(s: string): string {
  return s === '' ? '-' : s;
}

/** Rust usage page: `if r.email.is_empty() { "(不明)" }`. */
export function unknownIfEmpty(s: string): string {
  return s === '' ? '(不明)' : s;
}
