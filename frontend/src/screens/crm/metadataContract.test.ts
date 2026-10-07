// 境界の契約: Rust の /api/crm/metadata (src/handlers/crm_metadata.rs の deals 許可リスト) と、
// 架電結果の入力欄が必要とする Deal プロパティ (callResultModel.ts) が食い違っていないこと。
// 片方だけ変えると cargo test も他の vitest も通るのに、実データの入力欄だけが「入力欄を表示できません」になるため
import rustSource from '../../../../src/handlers/crm_metadata.rs?raw';
import { describe, expect, it } from 'vitest';
import { FIELD_PROPERTY, REQUIRED_DEFINITIONS, REQUIRED_TYPED_DEFINITIONS } from './callResultModel';

/** `"deals" => &[ ... ],` の中の文字列だけを取り出す */
function rustDealAllowlist(src: string): string[] {
  const m = /"deals"\s*=>\s*&\[([\s\S]*?)\]/.exec(src);
  if (!m?.[1]) throw new Error('deals allowlist not found in crm_metadata.rs');
  return [...m[1].matchAll(/"([^"]+)"/g)].map(x => x[1] ?? '');
}

describe('Rust metadata allowlist ↔ call-result form', () => {
  const allow = rustDealAllowlist(rustSource);
  it('parses a non-trivial list (guards against the regex silently matching nothing)', () => {
    expect(allow).toContain('dealstage');
    expect(allow.length).toBeGreaterThan(20);
  });
  it('every property the form reads definitions for is served by /api/crm/metadata', () => {
    const needed = [...new Set([...REQUIRED_DEFINITIONS, ...Object.keys(REQUIRED_TYPED_DEFINITIONS), ...Object.values(FIELD_PROPERTY)])].sort();
    expect(needed.filter(n => !allow.includes(n))).toEqual([]);
    expect(needed).toEqual(['bpo_10', 'bpo_13', 'bpo_14', 'bpo_16', 'bpo_23', 'bpo_3', 'bpo_33', 'bpo_4', 'bpo_40', 'bpo_42', 'bpo_45', 'bpo_57', 'bpo__']);
  });
});
