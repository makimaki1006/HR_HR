// Wording rules for UI text (correlation is not causation). The lists live in
// phrase_rules.json and must equal the Rust originals in
// src/handlers/insight/phrase_validator.rs (checked by phrases.test.ts).
import rules from './phrase_rules.json';

export const REQUIRED_PHRASES: readonly string[] = rules.required;
export const FORBIDDEN_PHRASES: readonly string[] = rules.forbidden;

/** First forbidden phrase contained in `text`, or null. */
export function findForbiddenPhrase(text: string): string | null {
  return FORBIDDEN_PHRASES.find((p) => text.includes(p)) ?? null;
}

/** Throws when `text` contains a forbidden assertive phrase (same rule as the Rust validator). */
export function assertNoForbiddenPhrase(text: string): void {
  const hit = findForbiddenPhrase(text);
  if (hit !== null) {
    throw new Error(
      `Forbidden phrase '${hit}' detected (correlation must not be stated as causation)`,
    );
  }
}

/**
 * Vitest helper: fails when `text` contains a forbidden assertive phrase.
 * Same rule as assertNoForbiddenPhrase; named for use in screen tests.
 */
export function assertValidPhrase(text: string): void {
  assertNoForbiddenPhrase(text);
}

/**
 * Optional check for analysis wording: true when `text` has at least one hedge phrase
 * (REQUIRED_PHRASES, e.g. 傾向 / 可能性). Not for labels or notes.
 */
export function hasHedgePhrase(text: string): boolean {
  return REQUIRED_PHRASES.some((p) => text.includes(p));
}
