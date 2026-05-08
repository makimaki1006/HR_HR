// -*- mode: typescript -*-
// Round 2.9-A: page.pdf() 直前の ECharts resize hook
//
// 真因 (Round 2.8-D):
//   page.pdf() (Chromium DevTools Page.printToPDF) は beforeprint /
//   afterprint / matchMedia('print') を発火させない仕様。
//   そのため src/.../helpers.rs:1040-1076 の resize hook が
//   page.pdf() 経路では動かず、container は screen viewport 幅
//   (~960pt) のまま PDF 化され、SVG attribute が本文域 (555pt)
//   を 405pt 超過する。
//
// 対策:
//   page.pdf() 呼出直前に明示的に DOM 操作 + ECharts.resize() を発火させ、
//   container の bbox.width が PDF 本文域 (760pt 安全枠) 以下に収まる
//   ことを wait_for_function で保証する。
//
// 注意:
//   - print CSS 変更 (Round 2.9-B) のスコープ外
//   - Rust source は変更しない (helpers.rs 既存 hook は window.print()
//     経路で引き続き動作)
//   - アクションバー印刷 (window.print()) 経路は本 helper 不要
import type { Page } from '@playwright/test';

/**
 * page.pdf() 直前に呼び出して ECharts container を A4 本文域に
 * 強制リサイズする。emulateMedia({ media: 'print' }) の直後に呼ぶこと。
 *
 * @param page - Playwright Page
 * @param maxWidthPt - container bbox.width の上限 (pt)。既定 760pt
 *                    (A4 595pt + 余裕枠。本文域 ~555pt より緩く設定)。
 */
export async function preparePdfRender(page: Page, maxWidthPt: number = 760): Promise<void> {
  // 1. DOM レベル: pdf-rendering クラス + container 幅を明示的に縮める
  await page.evaluate(() => {
    document.documentElement.classList.add('pdf-rendering');
    const charts = Array.from(document.querySelectorAll('[_echarts_instance_]')) as HTMLElement[];
    charts.forEach((el) => {
      el.style.width = '100%';
      el.style.maxWidth = '100%';
      // ECharts instance を取得して resize() 発火
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const echarts = (window as any).echarts;
      const instance = echarts?.getInstanceByDom?.(el);
      if (instance && typeof instance.resize === 'function') {
        try {
          instance.resize();
        } catch (_e) {
          // resize 失敗時は無視 (チャート初期化前の可能性)
        }
      }
    });
  });

  // 2. resize 反映待ち (非同期描画 + ResizeObserver の伝播)
  await page.waitForTimeout(800);

  // 3. 全 chart container の bbox.width が閾値以下であることを保証
  await page.waitForFunction(
    (limit: number) => {
      const charts = Array.from(document.querySelectorAll('[_echarts_instance_]')) as HTMLElement[];
      if (charts.length === 0) return true;
      return charts.every((el) => {
        const rect = el.getBoundingClientRect();
        return rect.width > 0 && rect.width <= limit;
      });
    },
    maxWidthPt,
    { timeout: 10_000 },
  );
}
