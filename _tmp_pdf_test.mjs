import { chromium } from '@playwright/test';
import * as path from 'path';
import * as fs from 'fs';

const BASE = 'https://hr-hw.onrender.com';
const EMAIL = 's_fujimaki@f-a-c.co.jp';
const PASS = 'fac_2026';
const FIXTURE = path.resolve('.playwright-mcp/indeed-2026-05-12.csv');
const OUT_DIR = 'C:/Users/fuji1/OneDrive/Pythonスクリプト保管/job_medley_project/.playwright-mcp';

(async () => {
  const browser = await chromium.launch({ headless: true });
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  page.setDefaultTimeout(120_000);

  console.log('[1/5] Login');
  await page.goto(`${BASE}/login`);
  await page.fill('input[type=email]', EMAIL);
  await page.fill('input[type=password]', PASS);
  await Promise.all([page.waitForNavigation({ waitUntil: 'domcontentloaded' }), page.click('button')]);

  console.log('[2/5] Open 媒体分析 tab');
  // navigate URL pattern works for tab via querystring (uses ?tab=...)
  // simpler: click tab
  await page.evaluate(() => {
    const t = Array.from(document.querySelectorAll('[role=tab]')).find(x => x.textContent.includes('媒体分析'));
    t && t.click();
  });
  await page.waitForTimeout(6000);

  console.log('[3/5] Upload CSV');
  // click ファイルを選択 label and set files via input
  await page.waitForSelector('input[type=file]', { timeout: 30000, state: 'attached' });
  const inp = await page.$('input[type=file]');
  await inp.setInputFiles(FIXTURE);
  await page.waitForTimeout(30000);

  console.log('[4/5] Get report URL');
  let reportUrl = null;
  for (let i = 0; i < 12; i++) {
    reportUrl = await page.evaluate(() => {
      const a = Array.from(document.querySelectorAll('a')).find(x => x.href.includes('/report/survey'));
      return a ? a.href : null;
    });
    if (reportUrl) break;
    console.log(`  waiting... (${i+1}/12)`);
    await page.waitForTimeout(10000);
  }
  if (!reportUrl) {
    console.error('Report URL not found after 2min');
    const body = await page.evaluate(() => document.body.innerText.slice(0, 1500));
    console.error('Body sample:', body);
    await browser.close();
    process.exit(1);
  }
  console.log('  URL:', reportUrl);

  await page.goto(reportUrl, { waitUntil: 'networkidle', timeout: 180_000 });
  await page.waitForTimeout(5000);

  console.log('[5/5] Generate PDF');
  await page.emulateMedia({ media: 'print' });
  await page.waitForTimeout(2000);

  const pdfPath = path.join(OUT_DIR, 'navy_report.pdf');
  await page.pdf({
    path: pdfPath,
    format: 'A4',
    printBackground: true,
    margin: { top: '14mm', right: '14mm', bottom: '16mm', left: '14mm' },
    displayHeaderFooter: false,
  });
  const size = fs.statSync(pdfPath).size;
  console.log(`  saved: ${pdfPath} (${(size/1024).toFixed(1)} KB)`);

  await browser.close();
})();
