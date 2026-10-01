import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import rules from '../generated/phrase_rules.json';
import {
  FORBIDDEN_PHRASES,
  REQUIRED_PHRASES,
  SURVEY_FORBIDDEN_PHRASES,
  assertNoForbiddenPhrase,
  assertNoSurveyForbiddenPhrase,
  assertValidPhrase,
  findForbiddenPhrase,
  hasHedgePhrase,
} from './phrases';

const RUST_SOURCE = fileURLToPath(
  new URL('../../../src/handlers/insight/phrase_validator.rs', import.meta.url),
);
const SURVEY_SOURCE = fileURLToPath(
  new URL(
    '../../../src/handlers/survey/report_html/mod.rs',
    import.meta.url,
  ),
);

/** Drops `//` comment text (a quoted word in a comment must not count as an element). */
function stripRustLineComments(source: string): string {
  return source
    .split('\n')
    .map((line) => line.replace(/\/\/.*$/, ''))
    .join('\n');
}

/** Extracts the string literals of `const NAME: &[&str] = &[ ... ];` or `[&str; N] = [ ... ];`. */
function rustStringArray(source: string, name: string): string[] {
  const block = new RegExp(
    String.raw`const ${name}: (?:&\[&str\] = &|\[&str; \d+\] = )\[([\s\S]*?)\];`,
  ).exec(source);
  if (!block?.[1]) throw new Error(`${name} not found`);
  return [...stripRustLineComments(block[1]).matchAll(/"([^"]*)"/g)].map((m) => m[1] ?? '');
}

describe('rustStringArray', () => {
  it('ignores quoted words inside // comments and reads both array forms', () => {
    const src = [
      'const A: &[&str] = &[',
      '    "x", // "ignored" comment',
      '    // "also ignored",',
      '    "y",',
      '];',
      'const B: [&str; 2] = [',
      '    "p",',
      '    "q",',
      '];',
    ].join('\n');
    expect(rustStringArray(src, 'A')).toEqual(['x', 'y']);
    expect(rustStringArray(src, 'B')).toEqual(['p', 'q']);
  });
});

describe('generated phrase_rules.json vs the Rust sources', () => {
  it('FORBIDDEN_PHRASES are identical (same order)', () => {
    const source = readFileSync(RUST_SOURCE, 'utf-8');
    expect(rustStringArray(source, 'FORBIDDEN_PHRASES')).toEqual(rules.forbidden);
    expect(rules.forbidden).toHaveLength(7);
  });

  it('REQUIRED_PHRASES are identical (same order)', () => {
    const source = readFileSync(RUST_SOURCE, 'utf-8');
    expect(rustStringArray(source, 'REQUIRED_PHRASES')).toEqual(rules.required);
    expect(rules.required).toHaveLength(8);
  });

  it('survey SURVEY_FORBIDDEN_WORDS are identical (same order)', () => {
    const source = readFileSync(SURVEY_SOURCE, 'utf-8');
    expect(rustStringArray(source, 'SURVEY_FORBIDDEN_WORDS')).toEqual(rules.survey_forbidden);
    expect(rules.survey_forbidden).toHaveLength(6);
  });
});

describe('assertNoForbiddenPhrase', () => {
  it('throws with the offending phrase for every forbidden phrase', () => {
    for (const phrase of FORBIDDEN_PHRASES) {
      expect(() => {
        assertNoForbiddenPhrase(`この施策は${phrase}効果がある`);
      }).toThrow(`Forbidden phrase '${phrase}' detected`);
    }
  });

  it('accepts hedged wording', () => {
    expect(() => {
      assertNoForbiddenPhrase('求人数が多い地域ほど給与が高い傾向が見られます');
    }).not.toThrow();
    expect(REQUIRED_PHRASES).toContain('傾向');
  });

  it('findForbiddenPhrase returns the first hit or null', () => {
    expect(findForbiddenPhrase('必ず100%成功')).toBe('必ず');
    expect(findForbiddenPhrase('可能性があります')).toBeNull();
  });

  it('assertValidPhrase mirrors validate_insight_phrase: forbidden or no hedge both throw', () => {
    expect(() => {
      assertValidPhrase('この施策で必ず改善する傾向がある');
    }).toThrow(/Forbidden phrase '必ず'/);
    expect(() => {
      assertValidPhrase('件数は120件です');
    }).toThrow(/Missing required hedging phrase/);
    expect(() => {
      assertValidPhrase('改善する傾向がみられる');
    }).not.toThrow();
    // The forbidden-only check still accepts un-hedged labels.
    expect(() => {
      assertNoForbiddenPhrase('件数は120件です');
    }).not.toThrow();
  });

  it('assertNoSurveyForbiddenPhrase throws for every survey forbidden word', () => {
    expect(SURVEY_FORBIDDEN_PHRASES).toContain('断言');
    for (const w of SURVEY_FORBIDDEN_PHRASES) {
      expect(() => {
        assertNoSurveyForbiddenPhrase(`この結果は${w}できる`);
      }).toThrow(`Survey forbidden phrase '${w}' detected`);
    }
    expect(() => {
      assertNoSurveyForbiddenPhrase('改善する可能性があります');
    }).not.toThrow();
  });

  it('hasHedgePhrase is true only when a REQUIRED phrase is present', () => {
    expect(hasHedgePhrase('増加する傾向')).toBe(true);
    expect(hasHedgePhrase('件数は120件')).toBe(false);
  });
});
