// Invoked only by the authenticated Rust report handler, with a private local HTML file.
const [executablePath, input, output, modulePath] = process.argv.slice(2);
const { chromium } = require(modulePath);

(async () => {
  const browser = await chromium.launch({ executablePath, headless: true, timeout: 15000 });
  try {
    const page = await browser.newPage({ viewport: { width: 1600, height: 1200 } });
    await page.goto(input, { waitUntil: 'load', timeout: 15000 });
    await page.evaluate(async () => { await document.fonts.ready; });
    await page.waitForFunction(() => document.documentElement.dataset.pdfReady === 'true', null, { timeout: 5000 });
    await page.emulateMedia({ media: 'print' });
    await page.evaluate(() => fitPages());
    const bounds = await page.evaluate(() => [...document.querySelectorAll('.pdf-content')].map(e => e.getBoundingClientRect().height));
    if (bounds.length !== 4 || bounds.some(height => height > 1045)) throw new Error('PDF page fitting failed');
    await page.pdf({ path: output, preferCSSPageSize: true, printBackground: true });
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error.message); process.exitCode = 1; });
