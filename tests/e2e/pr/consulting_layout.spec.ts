import { test, expect, type Page } from '@playwright/test';
import { readFileSync, mkdirSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { findLayoutProblems } from '../helpers/layout_check';
import { consultingFixture } from './helpers/consulting_layout_fixture';

const template = readFileSync(process.env.CS_LAYOUT_TEMPLATE || resolve(__dirname, '../../../templates/tabs/cs_dashboard.html'), 'utf8');
test.use({ actionTimeout: 10_000 });

const screens = ['deal/team', 'deal/today', 'deal/board', 'research/customer', 'research/focus', 'monthly/renewalpipe', 'research/phone', 'monthly/results', 'monthly/trust', 'deal/byowner', 'deal/detail'];
const scenarios = [
  { label: 'empty', count: 0, months: 0, long: false },
  { label: 'first-week', count: 1, months: 1, long: true },
  { label: 'normal', count: 6, months: 12, long: false },
  { label: 'many-long', count: 80, months: 30, long: true },
];
async function check(page: Page, label: string) {
  await expect(page.locator('#cs-main')).toHaveAttribute('aria-busy', 'false');
  await expect(page.locator('#cs-error')).toBeHidden();
  await expect(page.locator('#cs-main')).not.toContainText(/評価ではありません|読むときの注意|並べ方の決まり/);
  const headings = await page.locator('#cs-main h2').allTextContents();
  expect(headings.filter(title => /か[？?]?$/.test(title.trim()))).toEqual([]);
  await page.evaluate(() => { document.querySelectorAll('details').forEach(d => d.open = true); window.scrollTo(0, 0); });
  await page.waitForTimeout(250); // 図の幅の再計測と描き直しを待つ
  // 重なりの検査は画面に映っている範囲だけなので、上から下まで 1 画面ずつずらして調べ、結果を足し合わせる
  const height = await page.evaluate(() => document.documentElement.scrollHeight);
  const step = await page.evaluate(() => innerHeight);
  const report: Awaited<ReturnType<typeof findLayoutProblems>> = { overlaps: [], spills: [], svgClipped: [], pageOverflow: false };
  const seen = new Set<string>();
  for (let y = 0; y < height; y += step) {
    await page.evaluate(top => { window.scrollTo(0, top); }, y);
    const part = await findLayoutProblems(page, '.wrap');
    for (const key of ['overlaps', 'spills', 'svgClipped'] as const) for (const item of part[key]) if (!seen.has(key + item.text)) { seen.add(key + item.text); report[key].push(item); }
    report.pageOverflow ||= part.pageOverflow;
  }
  await page.evaluate(() => { window.scrollTo(0, 0); });
  await test.info().attach(label, { body: JSON.stringify(report, null, 2), contentType: 'application/json' });
  if (process.env.CS_LAYOUT_SHOTS && (!process.env.CS_LAYOUT_SHOT_MATCH || new RegExp(process.env.CS_LAYOUT_SHOT_MATCH).test(label))) {
    mkdirSync(process.env.CS_LAYOUT_SHOTS, { recursive: true });
    await page.screenshot({ path: join(process.env.CS_LAYOUT_SHOTS, `${label.replaceAll('/', '-')}.png`), fullPage: true });
  }
  expect.soft(report, label).toEqual({ overlaps: [], spills: [], svgClipped: [], pageOverflow: false });
}
for (const width of [1440, 1280]) for (const scenario of scenarios) {
  test(`コンサルKPIの文字が重ならず枠に収まる ${width} ${scenario.label}`, async ({ page }) => {
    test.setTimeout(180_000);
    await page.setViewportSize({ width, height: 900 });
    const fixture = consultingFixture(scenario.count, scenario.months, scenario.long);
    await page.route('**/consulting', r => r.fulfill({ contentType: 'text/html', body: template.replaceAll('{{ user }}', '架空の利用者') }));
    await page.route('**/api/consulting/**', r => { const u = new URL(r.request().url()); return r.fulfill({ json: fixture.api(u.pathname, u.searchParams) }); });
    for (const screen of screens) {
      await page.goto(`http://localhost/consulting#${screen}`);
      await check(page, `${width}-${scenario.label}-${screen}`);
      // 表の並べ替え、入力による絞り込み、選択、画面内タブを実際に操作して再検査する。
      const selectors = ['#cs-main th button', '#cs-main select', '#cs-main input[type="search"]', '#cs-main button[aria-pressed]', '#cs-censor', '#cs-focus', '#cs-main input[type="checkbox"]'];
      for (const selector of selectors) {
        const n = await page.locator(selector).count();
        const thorough = scenario.long && (selector.includes('select') || selector.includes('th button') || selector.includes('aria-pressed'));
        for (let i = 0; i < Math.min(n, thorough ? n : 1); i++) {
          const control = page.locator(selector).nth(i);
          if (!await control.count() || !await control.isVisible()) continue;
          if (selector.includes('select')) { const options = control.locator('option'); if (await options.count() > 1) await control.selectOption({ index: 1 }); }
          else if (selector.includes('search')) {
            await control.fill('架空');
            await page.waitForTimeout(400); // 検索の遅延処理が描き直してから畳みを開く
          }
          else if ((await control.getAttribute('type')) === 'checkbox') await control.setChecked(!await control.isChecked());
          else await control.click();
          await check(page, `${width}-${scenario.label}-${screen}-操作-${selectors.indexOf(selector)}-${i}`);
        }
      }
    }
    if (scenario.count) {
      for (const hash of ['research/customer?houjin=sample-0', 'deal/detail?id=sample-0']) {
        await page.goto(`http://localhost/consulting#${hash}`);
        await check(page, `${width}-${scenario.label}-${hash.split('?')[0]}-中身`);
        const switches = await page.locator('#cs-main input[type="checkbox"][id], #dd-ctl button[id]').evaluateAll(els => els.map(e => e.id));
        for (const id of switches.slice(0, 6)) {
          await page.locator(`[id="${id}"]`).click();
          await check(page, `${width}-${scenario.label}-${hash.split('?')[0]}-${id}`);
        }
      }
    }
  });
}
test('重なり・はみ出し・図の切れを意図的に作ると検出する', async ({ page }) => {
  await page.setContent('<div id="probe" style="position:relative;width:100px"><span style="position:absolute;top:0">重なる文字</span><b style="position:absolute;top:0">重なる文字</b><div style="width:20px">ABCDEFGHIJKLMNOPQRSTUVWXYZ</div><div style="width:2000px;height:1px"></div><svg width="50" height="50"><text x="-20" y="20">切れる文字</text></svg></div>');
  const report = await findLayoutProblems(page, '#probe');
  // 逆証明: この環境変数を付けると、通常の「問題0件」の判定が確実に失敗する。
  if (process.env.CS_LAYOUT_EXPECT_CLEAN_PROBE) expect(report).toEqual({ overlaps: [], spills: [], svgClipped: [], pageOverflow: false });
  expect(report.overlaps.length).toBeGreaterThan(0);
  expect(report.spills.length).toBeGreaterThan(0);
  expect(report.svgClipped.length).toBeGreaterThan(0);
  expect(report.pageOverflow).toBe(true);
});
