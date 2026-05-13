/**
 * ローカル page.pdf() で本番 HTML + style.rs 修正前/後を比較する検証 spec
 *
 * 実行:
 *   $env:BASE_URL="https://hr-hw.onrender.com"; $env:E2E_EMAIL="..."; $env:E2E_PASS="...";
 *   npx playwright test _local_pdf_verify --project=chromium
 */
import { test } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { loginAndUpload, buildReportUrl } from './helpers/session';

const FIXTURE = path.resolve(__dirname, 'fixtures', 'jobbox_test_1000.csv');
const OUT_DIR = path.resolve(__dirname, '..', '..', 'out', 'echart_local');

// style.rs 修正案 (overflow: visible + height: revert)
const FIX_CSS = `
  @media print {
    [_echarts_instance_] { overflow: visible !important; }
    [_echarts_instance_] svg, [_echarts_instance_] canvas { height: revert !important; }
    .echart svg, .echart canvas,
    .echart-wrap svg, .echart-wrap canvas,
    .echart-container svg, .echart-container canvas { height: revert !important; }
  }
  html.pdf-rendering [_echarts_instance_],
  html.pdf-rendering .echart,
  html.pdf-rendering .echart-wrap,
  html.pdf-rendering .echart-container { overflow: visible !important; }
  html.pdf-rendering .echart svg, html.pdf-rendering .echart canvas,
  html.pdf-rendering [_echarts_instance_] svg, html.pdf-rendering [_echarts_instance_] canvas {
    height: revert !important;
  }
`;

test.describe('local page.pdf() vs style.rs fix', () => {
  test('before and after CSS fix', async ({ page, browserName }) => {
    test.skip(browserName !== 'chromium', 'PDF generation requires Chromium');
    test.setTimeout(420_000);

    fs.mkdirSync(OUT_DIR, { recursive: true });

    const { sessionId } = await loginAndUpload(page, FIXTURE, 'jobbox');
    const base = process.env.BASE_URL ?? 'https://hr-hw.onrender.com';
    const url = base + buildReportUrl(sessionId, 'market_intelligence');

    // === 1. 修正前 PDF ===
    await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 120_000 });
    await page.waitForTimeout(4000);

    // SVG / 親要素 / .echart container の状態を測定
    const before = await page.evaluate(() => {
      const svgs = Array.from(document.querySelectorAll('svg'));
      const echartDivs = Array.from(document.querySelectorAll('.echart'));
      return {
        svgs: svgs.slice(0, 20).map((s) => ({
          attrH: s.getAttribute('height'),
          bbH: s.getBoundingClientRect().height,
          parentOv: s.parentElement ? getComputedStyle(s.parentElement).overflow : null,
        })),
        echartDivs: echartDivs.slice(0, 20).map((el) => ({
          inlineStyleHeight: (el as HTMLElement).style.height,
          clientH: (el as HTMLElement).clientHeight,
          bbH: el.getBoundingClientRect().height,
          computedH: getComputedStyle(el).height,
          parentTag: el.parentElement?.tagName,
          parentClass: el.parentElement?.className,
          parentBbH: el.parentElement?.getBoundingClientRect().height,
        })),
      };
    });
    console.log('STATE BEFORE fix:', JSON.stringify(before, null, 2));

    // pyramid 専用検査: ピラミッドを含む div を見つけて中身を解析
    const pyramidInfo = await page.evaluate(() => {
      const divs = Array.from(document.querySelectorAll('.echart'));
      const out: any[] = [];
      for (let i = 0; i < divs.length; i++) {
        const el = divs[i] as HTMLElement;
        const cfg = el.getAttribute('data-chart-config') ?? '';
        if (cfg.includes('男性') && cfg.includes('女性')) {
          out.push({
            idx: i,
            inlineH: el.style.height,
            clientH: el.clientHeight,
            bbH: el.getBoundingClientRect().height,
            svgH: el.querySelector('svg')?.getAttribute('height'),
            svgBbH: el.querySelector('svg')?.getBoundingClientRect().height,
            cfgSnippet: cfg.substring(0, 200),
          });
        }
      }
      return out;
    });
    console.log('PYRAMID DIVS:', JSON.stringify(pyramidInfo, null, 2));

    // === 1A: emulate print + pdf-rendering class (本番経路) ===
    await page.emulateMedia({ media: 'print' });
    await page.evaluate(() => document.documentElement.classList.add('pdf-rendering'));
    await page.waitForTimeout(800);
    await page.pdf({
      path: path.join(OUT_DIR, 'mi_prod_before.pdf'),
      format: 'A4',
      printBackground: true,
      preferCSSPageSize: true,
    });
    console.log('saved mi_prod_before.pdf (emulate=print + pdf-rendering class)');

    // === 1B: emulate なし + pdf-rendering class なし (screen mode で PDF) ===
    await page.emulateMedia({ media: 'screen' });
    await page.evaluate(() => document.documentElement.classList.remove('pdf-rendering'));
    await page.waitForTimeout(500);
    await page.pdf({
      path: path.join(OUT_DIR, 'mi_screen.pdf'),
      format: 'A4',
      printBackground: true,
      preferCSSPageSize: false,
    });
    console.log('saved mi_screen.pdf (no print emulate, no pdf-rendering)');

    // === 1C: emulate print のみ (pdf-rendering class なし) ===
    await page.emulateMedia({ media: 'print' });
    await page.waitForTimeout(500);
    await page.pdf({
      path: path.join(OUT_DIR, 'mi_print_no_class.pdf'),
      format: 'A4',
      printBackground: true,
      preferCSSPageSize: true,
    });
    console.log('saved mi_print_no_class.pdf (emulate=print, no pdf-rendering)');

    // print emulate 状態で SVG <text> 要素の computed style を測定
    await page.emulateMedia({ media: 'print' });
    await page.waitForTimeout(500);
    const textAnalysis = await page.evaluate(() => {
      const divs = Array.from(document.querySelectorAll('.echart'));
      const out: any[] = [];
      for (const el of divs) {
        const cfg = el.getAttribute('data-chart-config') ?? '';
        if (!cfg.includes('男性') || !cfg.includes('女性')) continue;
        const svg = el.querySelector('svg');
        if (!svg) continue;
        const texts = Array.from(svg.querySelectorAll('text'));
        out.push({
          svgAttrH: svg.getAttribute('height'),
          svgBbH: svg.getBoundingClientRect().height,
          divInlineH: (el as HTMLElement).style.height,
          divBbH: el.getBoundingClientRect().height,
          totalTexts: texts.length,
          allTextContents: texts.map((t) => t.textContent),
        });
      }
      return out;
    });
    console.log('PYRAMID SVG <text> in PRINT mode:', JSON.stringify(textAnalysis, null, 2));

    // print mode で SVG <text> に effective に当たっている全 CSS property を確認
    const printTextCss = await page.evaluate(() => {
      const svgs = Array.from(document.querySelectorAll('.echart svg'));
      const result: any[] = [];
      for (const svg of svgs.slice(0, 5)) {
        const cfg = svg.parentElement?.getAttribute('data-chart-config') ?? '';
        if (!cfg.includes('男性') && !cfg.includes('女性')) continue;
        const texts = Array.from(svg.querySelectorAll('text')).filter((t) => {
          const tx = t.textContent ?? '';
          // X 軸数値のみ
          return /^-?[\d,]+$/.test(tx);
        });
        result.push({
          xAxisTextCount: texts.length,
          samples: texts.slice(0, 3).map((t) => {
            const cs = getComputedStyle(t);
            const bb = t.getBoundingClientRect();
            return {
              text: t.textContent,
              bbX: bb.x, bbY: bb.y, bbW: bb.width, bbH: bb.height,
              cssDisplay: cs.display,
              cssVisibility: cs.visibility,
              cssOpacity: cs.opacity,
              cssFontSize: cs.fontSize,
              cssFontFamily: cs.fontFamily,
              cssFill: cs.fill,
              cssColor: cs.color,
              cssClipPath: cs.clipPath,
              cssTransform: cs.transform,
              cssAll: cs.all,
              attrTransform: t.getAttribute('transform'),
            };
          }),
        });
      }
      return result;
    });
    console.log('PRINT mode X-axis text CSS:', JSON.stringify(printTextCss, null, 2));

    // 累積で何個の @media print rule が削除されたか + 残っている stylesheet 情報
    const sheetsInfo = await page.evaluate(() => {
      return Array.from(document.styleSheets).map((s) => {
        try {
          return {
            href: s.href ? s.href.substring(0, 80) : '(inline)',
            ruleCount: (s as CSSStyleSheet).cssRules?.length ?? 'cross-origin',
            mediaCount: (s as CSSStyleSheet).cssRules
              ? Array.from((s as CSSStyleSheet).cssRules).filter(
                  (r) => r.type === CSSRule.MEDIA_RULE,
                ).length
              : 'N/A',
          };
        } catch (e) {
          return { href: s.href ? s.href.substring(0, 80) : '(inline)', error: String(e).substring(0, 60) };
        }
      });
    });
    console.log('STYLESHEETS after delete:', JSON.stringify(sheetsInfo, null, 2));

    await page.emulateMedia({ media: 'screen' });

    // === 1E: beforeprint hook 無効化 で PDF (true root cause hypothesis) ===
    // reset state
    await page.emulateMedia({ media: 'screen' });
    await page.waitForTimeout(300);
    // beforeprint / afterprint / resize event listener を全て差し替える
    await page.evaluate(() => {
      // window 全レベルの hook を一時的に上書き
      const w = window as any;
      const noop = () => {};
      // override addEventListener for 'beforeprint' onwards
      w.__originalAddListener = w.__originalAddListener || window.addEventListener.bind(window);
      // 既存 listener を removeable に無理矢理する代わりに、addEventListener 自体を上書き
      // ただ既存 listener は除去できないので、matchMedia print の change listener も含めて
      // chart.resize 経路を断つ手段: dispatchEvent をブロック (PDF 生成中の resize 抑止)
      const origResize = Element.prototype.dispatchEvent;
      // ECharts instance 全てを取り出して dispose 無しで resize メソッドを no-op に
      const echarts = w.echarts;
      if (echarts) {
        document.querySelectorAll('[_echarts_instance_]').forEach((el) => {
          const inst = echarts.getInstanceByDom(el);
          if (inst) {
            inst.resize = noop;
          }
        });
        console.log('chart.resize() neutralized on all ECharts instances');
      }
    });
    await page.emulateMedia({ media: 'print' });
    await page.waitForTimeout(800);
    await page.pdf({
      path: path.join(OUT_DIR, 'mi_print_no_resize.pdf'),
      format: 'A4',
      printBackground: true,
      preferCSSPageSize: true,
    });
    console.log('saved mi_print_no_resize.pdf (chart.resize disabled)');

    // === 1F: @media print 内の全ルールを動的に disable (二分探索 step 1) ===
    // 既存 style.rs の @media print rules を JS で削除して、X 軸が出るか確認
    await page.evaluate(() => {
      const disabledRules: string[] = [];
      for (const sheet of Array.from(document.styleSheets)) {
        try {
          const rules = (sheet as CSSStyleSheet).cssRules;
          // 後ろから消すと index がずれない
          for (let i = rules.length - 1; i >= 0; i--) {
            const r = rules[i];
            if (r.type === CSSRule.MEDIA_RULE) {
              const mr = r as CSSMediaRule;
              if (mr.conditionText.includes('print')) {
                disabledRules.push(mr.conditionText);
                (sheet as CSSStyleSheet).deleteRule(i);
              }
            }
          }
        } catch (e) {
          // cross-origin sheet (CDN) は無視
        }
      }
      (window as any).__disabledMediaPrint = disabledRules;
      console.log('disabled @media print rules count:', disabledRules.length);
    });
    await page.emulateMedia({ media: 'print' });
    await page.waitForTimeout(500);
    await page.pdf({
      path: path.join(OUT_DIR, 'mi_print_revert.pdf'),
      format: 'A4',
      printBackground: true,
      preferCSSPageSize: true,
    });
    console.log('saved mi_print_revert.pdf (ALL @media print rules deleted)');

    // reset
    await page.emulateMedia({ media: 'screen' });
    await page.evaluate(() => document.documentElement.classList.add('pdf-rendering'));

    // === 2. 修正後 PDF (CSS inject) ===
    await page.emulateMedia({ media: 'screen' }); // reset
    await page.evaluate(() => document.documentElement.classList.remove('pdf-rendering'));
    await page.addStyleTag({ content: FIX_CSS });
    await page.waitForTimeout(500);

    const after = await page.evaluate(() => {
      const svgs = Array.from(document.querySelectorAll('svg'));
      return svgs.slice(0, 4).map((s) => ({
        attrH: s.getAttribute('height'),
        bbH: s.getBoundingClientRect().height,
        parentOv: s.parentElement ? getComputedStyle(s.parentElement).overflow : null,
      }));
    });
    console.log('SVG state AFTER css inject:', JSON.stringify(after, null, 2));

    await page.emulateMedia({ media: 'print' });
    await page.evaluate(() => document.documentElement.classList.add('pdf-rendering'));
    await page.waitForTimeout(800);

    await page.pdf({
      path: path.join(OUT_DIR, 'mi_prod_after.pdf'),
      format: 'A4',
      printBackground: true,
      preferCSSPageSize: true,
    });
    console.log('saved mi_prod_after.pdf');
  });
});
