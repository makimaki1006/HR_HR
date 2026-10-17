import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

// 実際のテンプレートの CSS をブラウザーで確かめる。外部サービス・顧客情報は使わない。
const template = readFileSync(resolve(__dirname, '../../templates/tabs/cs_dashboard.html'), 'utf8');
const css = template.split('<style>')[1].split('</style>')[0];

for (const scheme of ['light', 'dark'] as const) {
  test(`コンサルKPI: OS が ${scheme} でも明るく読みやすい`, async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.emulateMedia({ colorScheme: scheme });
    await page.setContent(`<html lang="ja" data-theme="${scheme}"><head><style>${css}</style></head><body>
      <div class="wrap"><div class="layout"><nav class="side"><h2>毎日</h2>
      <button aria-current="page">今日</button><button>案件一覧</button></nav><main>
      <h2 class="sec mincho">今日、確かめること</h2><h2 class="sec part">案件の状況</h2>
      <div class="kpis"><button class="kpi"><span class="lbl">稼働中</span><span class="big">604</span>
      <span class="fine">稼働中の案件のうち、接触が30日以上空いている案件を数えています</span><span class="act">案件一覧へ</span></button>
      <button class="kpi"><span class="lbl">今週満了する案件</span><span class="big">24</span><span class="fine">稼働中604件のうち</span></button></div>
      <table><thead><tr><th>案件名</th><th class="n">件数</th></tr></thead><tbody><tr><td>架空の案件 <span class="tag m">接触あり</span></td><td class="n">123</td></tr></tbody></table>
      <p class="muted small">担当者の評価ではありません</p>
      <details class="fold notefold"><summary>比べ方を見る</summary><p>接触が30日以上空いている案件を確認します</p></details>
      </main></div></div></body></html>`);
    await expect(page.locator('body')).toHaveCSS('background-color', 'rgb(244, 246, 248)');
    await expect(page.locator('html')).toHaveCSS('color-scheme', 'light');
    await expect(page.locator('body')).toHaveCSS('font-size', '14px');
    await expect(page.locator('h2.mincho')).toHaveCSS('font-size', '19px');
    await expect(page.locator('table')).toHaveCSS('font-size', '13px');
    await expect(page.locator('th').first()).toHaveCSS('font-size', '13px');
    await expect(page.locator('.small')).toHaveCSS('font-size', '12px');
    await expect(page.locator('.kpi .fine').first()).toHaveCSS('white-space', 'nowrap');
    await expect(page.locator('.kpi .fine').first()).toHaveCSS('text-overflow', 'ellipsis');
    await expect(page.locator('td.n')).toHaveCSS('text-align', 'right');
    await expect(page.locator('td.n')).toHaveCSS('font-variant-numeric', 'tabular-nums');
    const dimensions = await page.evaluate(() => ({
      cards: [...document.querySelectorAll('.kpi')].map(e => e.getBoundingClientRect().height),
      tag: document.querySelector('.tag')!.getBoundingClientRect().height,
      row: document.querySelector('tbody tr')!.getBoundingClientRect().height,
      summary: document.querySelector('summary')!.getBoundingClientRect().height,
    }));
    expect(dimensions.cards[0]).toBe(dimensions.cards[1]);
    expect(dimensions.tag).toBeGreaterThanOrEqual(18);
    expect(dimensions.tag).toBeLessThanOrEqual(20);
    expect(dimensions.row).toBeGreaterThanOrEqual(36);
    expect(dimensions.row).toBeLessThanOrEqual(40);
    expect(dimensions.summary).toBeLessThanOrEqual(26);
    // 地・注記・色つきの名札の組み合わせも 4.5:1 を保つ。
    const contrasts = await page.evaluate(() => {
      const lum = (color: string) => {
        const [r,g,b] = color.match(/[\d.]+/g)!.slice(0,3).map(Number).map(v => v / 255).map(v => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4);
        return .2126*r + .7152*g + .0722*b;
      };
      return ['body', '.small', '.fine', '.tag'].map(selector => {
        const el = document.querySelector(selector)!;
        const fg = lum(getComputedStyle(el).color);
        let bgEl: Element | null = el;
        while (bgEl && getComputedStyle(bgEl).backgroundColor === 'rgba(0, 0, 0, 0)') bgEl = bgEl.parentElement;
        const bg = lum(getComputedStyle(bgEl!).backgroundColor);
        return (Math.max(fg,bg)+.05)/(Math.min(fg,bg)+.05);
      });
    });
    contrasts.forEach(ratio => expect(ratio).toBeGreaterThanOrEqual(4.5));
    await page.locator('summary').focus();
    await expect(page.locator('summary')).toHaveCSS('outline-style', 'solid');
    await page.keyboard.press('Enter');
    await expect(page.locator('details')).toHaveAttribute('open', '');
  });
}
