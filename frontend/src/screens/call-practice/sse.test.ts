import { describe, expect, it } from 'vitest';
import { parseSse } from './sse';

describe('parseSse', () => {
  it('完結したイベントだけを取り出し、途中のものは持ち越す', () => {
    const a = parseSse('event: delta\ndata: {"text":"ドラ"}\n\nevent: delta\ndata: {"text":"イ');
    expect(a.events).toEqual([{ event: 'delta', data: { text: 'ドラ' } }]);
    const b = parseSse(`${a.rest}バー"}\n\nevent: done\ndata: {"ended":false}\n\n`);
    expect(b.events).toEqual([
      { event: 'delta', data: { text: 'イバー' } },
      { event: 'done', data: { ended: false } },
    ]);
    expect(b.rest).toBe('');
  });

  it('壊れた data は読み飛ばし、改行コードの違いを許す', () => {
    const r = parseSse('event: delta\r\ndata: {oops\r\n\r\nevent: error\r\ndata: {"error_kind":"llm_busy"}\r\n\r\n');
    expect(r.events).toEqual([{ event: 'error', data: { error_kind: 'llm_busy' } }]);
  });
});
