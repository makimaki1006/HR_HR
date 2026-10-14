// 境界の契約: Rust の /api/crm/metadata (src/handlers/crm_metadata.rs の deals 許可リスト) と、
// 停止の理由の表示ラベルに必要な Deal プロパティ (useResultDefinitions.ts) が食い違っていないこと。
// 片方だけ変えると cargo test も他の vitest も通るのに、実データの表示ラベルだけが読めなくなるため
import rustSource from '../../../../src/handlers/crm_metadata.rs?raw';
import { describe, expect, it } from 'vitest';
import { REQUIRED_DEFINITIONS } from './useResultDefinitions';

/** `"deals" => &[ ... ],` の中の文字列だけを取り出す */
function rustDealAllowlist(src: string): string[] {
  const m = /"deals"\s*=>\s*&\[([\s\S]*?)\]/.exec(src);
  if (!m?.[1]) throw new Error('deals allowlist not found in crm_metadata.rs');
  return [...m[1].matchAll(/"([^"]+)"/g)].map(x => x[1] ?? '');
}

describe('Rust metadata allowlist ↔ stop-reason labels', () => {
  const allow = rustDealAllowlist(rustSource);
  it('parses a non-trivial list (guards against the regex silently matching nothing)', () => {
    expect(allow).toContain('dealstage');
    expect(allow.length).toBeGreaterThan(20);
  });
  it('every property the stop labels read definitions for is served by /api/crm/metadata', () => {
    const needed = [...REQUIRED_DEFINITIONS].sort();
    expect(needed.filter(n => !allow.includes(n))).toEqual([]);
    expect(needed).toEqual(['bpo_10', 'bpo_4']);
  });
});
