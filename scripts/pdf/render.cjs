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
    const fits = await page.evaluate(() => {
      const panels = [...document.querySelectorAll('.pdf-content')];
      const expected = ['panel-excel', 'panel-google', 'panel-indeed', 'panel-population'];
      if (panels.length !== expected.length) return false;
      return panels.every((panel, index) => {
        if (panel.id !== expected[index]) return false;
        const frame = panel.closest('.pdf-page')?.getBoundingClientRect();
        if (!frame) return false;
        const within = rect => Number.isFinite(rect.width) && Number.isFinite(rect.height)
          && rect.left >= frame.left - 1 && rect.top >= frame.top - 1
          && rect.right <= frame.right + 1 && rect.bottom <= frame.bottom + 1;
        const box = panel.getBoundingClientRect();
        if (box.width <= 0 || box.height <= 0 || !within(box)) return false;
        const walker = document.createTreeWalker(panel, NodeFilter.SHOW_TEXT);
        while (walker.nextNode()) {
          const node = walker.currentNode;
          if (!node.textContent.trim() || node.parentElement.closest('script,style')) continue;
          const range = document.createRange();
          range.selectNodeContents(node);
          for (const rect of range.getClientRects()) {
            if (rect.width > 0 && rect.height > 0 && !within(rect)) return false;
          }
        }
        return [...panel.querySelectorAll('svg,img')].every(element => within(element.getBoundingClientRect()));
      });
    });
    if (!fits) throw new Error('PDF page fitting failed');
    await page.pdf({ path: output, preferCSSPageSize: true, printBackground: true });
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error.message); process.exitCode = 1; });
