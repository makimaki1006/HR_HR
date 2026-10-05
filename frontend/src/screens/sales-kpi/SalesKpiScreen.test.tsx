import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { DATA_TIMEOUT_MS, LoadStateView, SalesKpiScreen, dataPath } from './SalesKpiScreen';

describe('SalesKpiScreen', () => {
  it('読み込み中の表示から始まる (旧画面と同じ文言)', () => {
    const html = renderToStaticMarkup(<SalesKpiScreen />);
    expect(html).toBe('<div class="wrap"><div class="loadstate" id="loadstate">データを読み込んでいます…</div></div>');
  });
  it('エラー表示 (旧画面と同じ見出しと案内)', () => {
    const html = renderToStaticMarkup(<LoadStateView state={{ status: 'error', message: 'HTTP 503' }} />);
    expect(html).toContain('<div class="loadstate err" id="loadstate" role="alert"><b>データを読み込めませんでした。</b>HTTP 503<br/>');
    expect(html).toContain('スプレッドシートの KPI営業_ シートが揃っているか、毎朝の同期 (GitHub Actions の sales_kpi_daily) が動いているかを確認してください。');
  });
  it('GAS は撤去済みなので案内に出さない', () => {
    const html = renderToStaticMarkup(<LoadStateView state={{ status: 'error', message: 'x' }} />);
    expect(html).not.toContain('GAS');
  });
  it('待ち時間の上限は 90 秒 (デプロイ直後はシート 11 枚を読むので既定の 15 秒では切れる)', () => {
    expect(DATA_TIMEOUT_MS).toBe(90_000);
  });
  it('?refresh=1 のときだけ API にも refresh=1 を付ける', () => {
    expect(dataPath('')).toBe('/api/sales-kpi/data');
    expect(dataPath('?x=1')).toBe('/api/sales-kpi/data');
    expect(dataPath('?refresh=1')).toBe('/api/sales-kpi/data?refresh=1');
  });
});
