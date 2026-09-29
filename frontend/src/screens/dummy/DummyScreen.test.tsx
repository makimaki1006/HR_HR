import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { AppPingResponse } from '../../generated/AppPingResponse';
import { DummyScreen, PingView } from './DummyScreen';

describe('DummyScreen', () => {
  it('renders the fixed heading text and starts in the loading state', () => {
    const html = renderToStaticMarkup(<DummyScreen />);
    expect(html).toContain('<h1>React 基盤の疎通確認画面</h1>');
    expect(html).toContain('サーバに問い合わせ中…');
  });
});

describe('PingView', () => {
  it('shows the values of the generated AppPingResponse type', () => {
    const data: AppPingResponse = { message: 'pong', server_time: '2026-09-29T09:00:00Z' };
    const html = renderToStaticMarkup(<PingView state={{ status: 'ok', data }} />);
    expect(html).toContain('<dd data-testid="ping-message">pong</dd>');
    expect(html).toContain('<dd data-testid="ping-server-time">2026-09-29T09:00:00Z</dd>');
  });

  it('shows the error message', () => {
    const html = renderToStaticMarkup(
      <PingView state={{ status: 'error', message: 'HTTP 500' }} />,
    );
    expect(html).toContain('サーバ応答を取得できませんでした: HTTP 500');
  });
});
