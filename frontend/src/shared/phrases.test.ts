import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import rules from './phrase_rules.json';
import { FORBIDDEN_PHRASES, REQUIRED_PHRASES, assertNoForbiddenPhrase, findForbiddenPhrase } from './phrases';

const RUST_SOURCE = fileURLToPath(
  new URL('../../../src/handlers/insight/phrase_validator.rs', import.meta.url),
);

/** Extracts the string literals of `const NAME: &[&str] = &[ ... ];`. */
function rustStringArray(source: string, name: string): string[] {
  const block = new RegExp(`const ${name}: &\\[&str\\] = &\\[([\\s\\S]*?)\\];`).exec(source);
  if (!block?.[1]) throw new Error(`${name} not found in phrase_validator.rs`);
  return [...block[1].matchAll(/"([^"]*)"/g)].map((m) => m[1] ?? '');
}

describe('phrase_rules.json vs phrase_validator.rs', () => {
  const source = readFileSync(RUST_SOURCE, 'utf-8');

  it('FORBIDDEN_PHRASES are identical (same order)', () => {
    expect(rustStringArray(source, 'FORBIDDEN_PHRASES')).toEqual(rules.forbidden);
    expect(rules.forbidden).toHaveLength(7);
  });

  it('REQUIRED_PHRASES are identical (same order)', () => {
    expect(rustStringArray(source, 'REQUIRED_PHRASES')).toEqual(rules.required);
    expect(rules.required).toHaveLength(8);
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
});
