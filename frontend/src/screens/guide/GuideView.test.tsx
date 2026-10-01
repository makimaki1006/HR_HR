import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { GuideResponse } from '../../generated/GuideResponse';
import guideJson from '../../../../src/handlers/guide_content.json?raw';
import { GuideView } from './GuideView';

// The Rust side embeds this same file (src/handlers/guide_content.json) and serves it as GET /api/guide.
const data = JSON.parse(guideJson) as GuideResponse;

const count = (html: string, needle: string): number => html.split(needle).length - 1;

describe('GuideView with the real guide data', () => {
  const html = renderToStaticMarkup(<GuideView data={data} />);

  it('renders the same structure counts as the old /tab/guide', () => {
    expect(count(html, '<table')).toBe(17);
    expect(count(html, '<details')).toBe(17);
    expect(count(html, '<img')).toBe(6);
    expect(count(html, 'class="guide__card"')).toBe(9);
    expect(count(html, '<h5')).toBe(6);
  });

  it('renders concrete text values', () => {
    expect(html).toContain('<h2 class="guide__heading guide__heading--title">📖 取扱説明書</h2>');
    expect(html).toContain('この地域の求人市場の全体像');
    expect(html).toContain('📈 トレンド → 外部比較');
    expect(html).toContain('src="/static/guide/trend_tokyo.png"');
    expect(html).toContain('alt="トレンド: 東京都"');
    expect(html).toContain('HW掲載求人のみが対象です');
  });

  it('marks the first index column as half width and the last row without separator', () => {
    expect(html).toContain('guide__th guide__th--half');
    // 10 index rows -> 9 separators in that table; whole document has many more, so check the first table only
    const firstTable = html.slice(html.indexOf('<table'), html.indexOf('</table>'));
    expect(count(firstTable, 'guide__tr--sep')).toBe(9);
  });

  it('escapes text (React default), not raw HTML', () => {
    const evil: GuideResponse = {
      blocks: [{ kind: 'heading', style: 'title', text: '<script>alert(1)</script>' }],
    };
    const out = renderToStaticMarkup(<GuideView data={evil} />);
    expect(out).not.toContain('<script>');
    expect(out).toContain('&lt;script&gt;');
  });
});
