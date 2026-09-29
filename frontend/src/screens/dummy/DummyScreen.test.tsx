import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { DummyScreen } from './DummyScreen';

describe('DummyScreen', () => {
  it('renders the fixed heading text', () => {
    const html = renderToStaticMarkup(<DummyScreen />);
    expect(html).toContain('<h1>React 基盤の疎通確認画面</h1>');
  });
});
