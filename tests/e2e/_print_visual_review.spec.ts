import { test, expect } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { loginAndUpload, buildReportUrl } from './helpers/session';
import { preparePdfRender } from './helpers/pdf_helper';

test('Print/PDF P1 visual review artifact generation', async ({ page }) => {
  test.setTimeout(300_000);
  const outDir = path.join(process.cwd(), 'out', 'print_review_p1');
  fs.mkdirSync(outDir, { recursive: true });
  const csvPath = path.join(process.cwd(), 'tests', 'e2e', 'fixtures', 'indeed_test_50.csv');
  const { sessionId } = await loginAndUpload(page, csvPath);

  const themes = ['default', 'v8', 'v7a'];
  const allMetrics: Record<string, unknown> = {};

  for (const theme of themes) {
    await page.goto(buildReportUrl(sessionId, 'market_intelligence', theme), { waitUntil: 'domcontentloaded', timeout: 120_000 });
    await page.waitForSelector('.mi-print-summary, .mi-hero-bar, .mi-print-annotations', { state: 'attached', timeout: 60_000 });
    await page.emulateMedia({ media: 'print' });
    await page.locator('.mi-print-summary').first().waitFor({ state: 'visible', timeout: 30_000 });
    await page.waitForTimeout(500);

    const metrics = await page.evaluate(() => {
      const selectors = [
        '.mi-print-summary',
        '.mi-hero-bar',
        '.mi-parent-ranking',
        '.mi-print-annotations',
        // P2-Round6-B (2026-05-08): 架空 `.mi-section` から
        // 実 DOM 準拠の `[data-mi-section]` (全 MI section 取得) に置換。
        // 詳細: docs/SPEC_SELECTOR_AUDIT_2026_05_08.md §5 案 B
        '[data-mi-section]',
      ];
      const result: any = { url: location.href, scrollHeight: document.documentElement.scrollHeight, viewport: { w: innerWidth, h: innerHeight }, elements: [] };
      for (const selector of selectors) {
        const nodes = Array.from(document.querySelectorAll(selector)) as HTMLElement[];
        result.elements.push({
          selector,
          count: nodes.length,
          boxes: nodes.slice(0, 10).map((el) => {
            const r = el.getBoundingClientRect();
            const cs = getComputedStyle(el);
            return {
              text: (el.innerText || '').replace(/\s+/g, ' ').slice(0, 160),
              display: cs.display,
              visibility: cs.visibility,
              breakInside: cs.breakInside,
              pageBreakInside: (cs as any).pageBreakInside,
              x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height),
            };
          }),
        });
      }
      result.hardNgHits = ['推定人数','想定人数','母集団人数','target_count','estimated_population','estimated_worker_count','resident_population_estimate']
        .filter((term) => document.body.innerText.includes(term));
      result.personEstimatePattern = /\d+\s*人\s*見込み/.test(document.body.innerText);
      return result;
    });
    allMetrics[theme] = metrics;

    await page.screenshot({ path: path.join(outDir, `print_${theme}_fullpage.png`), fullPage: true });
    const html = await page.content();
    fs.writeFileSync(path.join(outDir, `print_${theme}.html`), html, 'utf8');

    if (theme === 'default') {
      // Round 2.9-A: page.pdf() 直前に ECharts container を A4 本文域に強制 resize
      await preparePdfRender(page);
      await page.pdf({ path: path.join(outDir, 'market_intelligence_print_default.pdf'), format: 'A4', printBackground: true });
    }
  }

  fs.writeFileSync(path.join(outDir, 'print_metrics.json'), JSON.stringify(allMetrics, null, 2), 'utf8');
  expect((allMetrics as any).default.hardNgHits.length).toBe(0);
  expect((allMetrics as any).default.personEstimatePattern).toBe(false);
});

