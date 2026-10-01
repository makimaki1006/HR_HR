import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { rankColor } from './panels';

// Panel 1 の rank_label は Rust の classify_difficulty が決める。色の対応表 (React の RANK_COLOR と
// 旧画面 templates/tabs/recruitment_diag.html の levelColor) が API の実際のラベルを全部引けること、
// 旧と新の対応表が同じであることを、両方のソースを読んで確かめる (2026-10-02: 最上位ラベルは
// API では「超激戦」なのに、対応表が「非常に激戦」をキーにしていて色が付かなかった)。
const RUST_HANDLERS = fileURLToPath(
  new URL('../../../../src/handlers/recruitment_diag/handlers.rs', import.meta.url),
);
const LEGACY_TEMPLATE = fileURLToPath(
  new URL('../../../../templates/tabs/recruitment_diag.html', import.meta.url),
);

/** classify_difficulty が返すラベル (データ不足系を除く)。 */
function apiRankLabels(): string[] {
  const src = readFileSync(RUST_HANDLERS, 'utf-8');
  const start = src.indexOf('fn classify_difficulty');
  const end = src.indexOf('\n}\n', start);
  const body = src.slice(start, end);
  const labels = [...body.matchAll(/^\s+"([^"]{1,12})",\s*$/gm)].map((m) => m[1] ?? '');
  return [...new Set(labels)].filter((l) => !l.includes('データ不足'));
}

/** 旧画面の levelColor のオブジェクトリテラル ({'ラベル': 'クラス', ...})。 */
function legacyLevelColor(): Record<string, string> {
  const src = readFileSync(LEGACY_TEMPLATE, 'utf-8');
  const m = /const levelColor = \(\{([\s\S]*?)\}\)\[rankLabel\]/.exec(src);
  if (!m) throw new Error('旧画面の levelColor が見つからない');
  const entries: [string, string][] = [...(m[1] ?? '').matchAll(/'([^']+)':\s*'([^']+)'/g)].map((p) => [
    p[1] ?? '',
    p[2] ?? '',
  ]);
  return Object.fromEntries(entries);
}

describe('Panel 1 のランク色', () => {
  it('API のラベルを 5 つ読み取れている (走査が壊れていない)', () => {
    expect(apiRankLabels().sort()).toEqual(['平均的', '激戦', '穏やか', '穴場（競合ほぼなし）', '超激戦'].sort());
  });

  it('API の全ラベルに既定色 (slate) 以外の色が付く', () => {
    for (const label of apiRankLabels()) {
      expect(rankColor(label), label).not.toBe('text-slate-300');
    }
    expect(rankColor('超激戦')).toBe('text-red-400');
  });

  it('旧画面の対応表と React の対応表が同じ色を返す (旧 == 新)', () => {
    const legacy = legacyLevelColor();
    for (const label of apiRankLabels()) {
      const old = legacy[label] ?? (label.startsWith('穴場') ? 'text-blue-400' : 'text-slate-300');
      expect(rankColor(label), label).toBe(old);
    }
  });
});
