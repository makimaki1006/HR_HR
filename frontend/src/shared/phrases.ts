// Wording rules for UI text (correlation is not causation). The lists are generated from the
// Rust sources into src/generated/phrase_rules.json (a Rust #[test] writes it) and must equal
// the Rust originals; phrases.test.ts also compares them with the Rust sources.
//   required / forbidden : src/handlers/insight/phrase_validator.rs
//   survey_forbidden     : FORBIDDEN_WORDS in src/handlers/survey/report_html/navy_report/sp_report.rs
import rules from '../generated/phrase_rules.json';

export const REQUIRED_PHRASES: readonly string[] = rules.required;
export const FORBIDDEN_PHRASES: readonly string[] = rules.forbidden;
/** Assertive / promising wording banned in the survey report (sp_report.rs FORBIDDEN_WORDS). */
export const SURVEY_FORBIDDEN_PHRASES: readonly string[] = rules.survey_forbidden;

/** First forbidden phrase contained in `text`, or null. */
export function findForbiddenPhrase(text: string): string | null {
  return FORBIDDEN_PHRASES.find((p) => text.includes(p)) ?? null;
}

/** Throws when `text` contains a forbidden assertive phrase (forbidden-word check only). */
export function assertNoForbiddenPhrase(text: string): void {
  const hit = findForbiddenPhrase(text);
  if (hit !== null) {
    throw new Error(
      `Forbidden phrase '${hit}' detected (correlation must not be stated as causation)`,
    );
  }
}

/** Throws when `text` contains a survey-report forbidden phrase (sp_report.rs FORBIDDEN_WORDS). */
export function assertNoSurveyForbiddenPhrase(text: string): void {
  const hit = SURVEY_FORBIDDEN_PHRASES.find((p) => text.includes(p));
  if (hit !== undefined) {
    throw new Error(`Survey forbidden phrase '${hit}' detected`);
  }
}

/**
 * Optional check for analysis wording: true when `text` has at least one hedge phrase
 * (REQUIRED_PHRASES, e.g. 傾向 / 可能性). Not for labels or notes.
 */
export function hasHedgePhrase(text: string): boolean {
  return REQUIRED_PHRASES.some((p) => text.includes(p));
}

/**
 * Same rule as Rust validate_insight_phrase: no forbidden phrase AND at least one REQUIRED
 * hedge phrase. For analysis sentences (insight bodies), not for labels or notes; use
 * assertNoForbiddenPhrase for those.
 */
export function assertValidPhrase(text: string): void {
  assertNoForbiddenPhrase(text);
  if (!hasHedgePhrase(text)) {
    throw new Error(
      `Missing required hedging phrase. Body must include one of: [${REQUIRED_PHRASES.map((p) => `"${p}"`).join(', ')}]`,
    );
  }
}
