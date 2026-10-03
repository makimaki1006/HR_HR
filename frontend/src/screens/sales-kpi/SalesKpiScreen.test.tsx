import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { LoadStateView, SalesKpiScreen, dataPath } from './SalesKpiScreen';

describe('SalesKpiScreen', () => {
  it('読み込み中の表示から始まる (旧画面と同じ文言)', () => {
    const html = renderToStaticMarkup(<SalesKpiScreen />);
    expect(html).toBe('<div class="wrap"><div class="loadstate" id="loadstate">データを読み込んでいます…</div></div>');
  });
  it('エラー表示 (旧画面と同じ見出しと案内)', () => {
    const html = renderToStaticMarkup(<LoadStateView state={{ status: 'error', message: 'HTTP 503' }} />);
    expect(html).toContain('<div class="loadstate err" id="loadstate" role="alert"><b>データを読み込めませんでした。</b>HTTP 503<br/>');
    expect(html).toContain('スプレッドシートの KPI営業_ シートが揃っているか、GAS の sales_kpi_sync が動いているかを確認してください。');
  });
  it('?refresh=1 のときだけ API にも refresh=1 を付ける', () => {
    expect(dataPath('')).toBe('/api/sales-kpi/data');
    expect(dataPath('?x=1')).toBe('/api/sales-kpi/data');
    expect(dataPath('?refresh=1')).toBe('/api/sales-kpi/data?refresh=1');
  });
});
