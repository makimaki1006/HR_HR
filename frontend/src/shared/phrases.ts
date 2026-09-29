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
