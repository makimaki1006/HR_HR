import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { FORBIDDEN_PHRASES } from '../shared/phrases';
import { NOTE_TEXT, Note, type NoteKind } from './Note';

describe('Note', () => {
  it('hw-scope states the HW-only scope', () => {
    const html = renderToStaticMarkup(<Note kind="hw-scope" />);
    expect(html).toBe(
      '<p class="hw-note" role="note" data-note-kind="hw-scope"><span>ハローワーク掲載求人のみが対象で、全求人市場ではありません。</span></p>',
    );
  });

  it('correlation states correlation is not causation', () => {
    const html = renderToStaticMarkup(<Note kind="correlation" />);
    expect(html).toContain('相関関係であり、因果関係を示すものではありません。');
    expect(html).toContain('data-note-kind="correlation"');
  });

  it('custom renders only its children', () => {
    const html = renderToStaticMarkup(<Note kind="custom">n が小さい地域は参考値です</Note>);
    expect(html).toBe(
      '<p class="hw-note" role="note" data-note-kind="custom">n が小さい地域は参考値です</p>',
    );
  });

  it('appends children after the fixed text', () => {
    const html = renderToStaticMarkup(<Note kind="hw-scope">2026年9月時点</Note>);
    expect(html).toContain('全求人市場ではありません。</span>2026年9月時点</p>');
  });

  it('no kind renders a forbidden phrase', () => {
    const kinds: NoteKind[] = ['hw-scope', 'correlation', 'source', 'custom'];
    for (const kind of kinds) {
      const html = renderToStaticMarkup(<Note kind={kind}>補足</Note>);
      for (const phrase of FORBIDDEN_PHRASES) {
        expect(html, `${kind} contains ${phrase}`).not.toContain(phrase);
      }
    }
    for (const text of Object.values(NOTE_TEXT)) {
      for (const phrase of FORBIDDEN_PHRASES) expect(text).not.toContain(phrase);
    }
  });

  it('source renders 出典: followed by the source name', () => {
    const html = renderToStaticMarkup(<Note kind="source">ハローワーク求人データ</Note>);
    expect(html).toBe(
      '<p class="hw-note" role="note" data-note-kind="source"><span>出典: </span>ハローワーク求人データ</p>',
    );
  });
});
