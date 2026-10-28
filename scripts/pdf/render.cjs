// Invoked only by the authenticated Rust report handler, with a private local HTML file.
const [executablePath, input, output, modulePath] = process.argv.slice(2);
const { chromium } = require(modulePath);

(async () => {
  const browser = await chromium.launch({ executablePath, headless: true, timeout: 15000 });
  try {
    // 印刷時の Chromium のレイアウト幅は 900px 弱で、画面用の @media(max-width:900px) が効く。
    // 縮小率と枠内の検査を印刷時と同じ条件で測るため、画面幅ではなく印刷に近い幅にする
    // (1600px で測ると 2 列のまま縮小率が決まり、印刷で 1 列になって下へ溢れても検出できなかった)。
    const page = await browser.newPage({ viewport: { width: 880, height: 1200 } });
    await page.goto(input, { waitUntil: 'load', timeout: 15000 });
    await page.evaluate(async () => { await document.fonts.ready; });
    await page.waitForFunction(() => document.documentElement.dataset.pdfReady === 'true', null, { timeout: 5000 });
    await page.emulateMedia({ media: 'print' });
    await page.evaluate(() => fitPages());
    const fits = await page.evaluate(() => {
      const panels = [...document.querySelectorAll('.pdf-content')];
      // 検索語の分析があるときだけ、Indeed の直後に別ページが入る。
      const withKeywords = panels.length === 6;
      const expected = withKeywords
        ? ['panel-excel', 'panel-keyword', 'panel-google', 'panel-indeed', 'panel-indeed-keywords', 'panel-population']
        : ['panel-excel', 'panel-keyword', 'panel-google', 'panel-indeed', 'panel-population'];
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
    // 上の検査は画面幅 (1600px) の配置を測る。印刷時は Chromium が別の幅でレイアウトし直すので、
    // 溢れは実際の PDF のページ数でも確かめる (1 パネル = 1 ページ。溢れると余分なページが増える)。
    // 検査に通ったときだけ出力ファイルを書く (呼び出し側はファイルができたら返すため)。
    const panelCount = await page.evaluate(() => document.querySelectorAll('.pdf-content').length);
    const pdf = await page.pdf({ preferCSSPageSize: true, printBackground: true });
    const pageCount = (pdf.toString('latin1').match(/\/Type\s*\/Page(?![a-zA-Z])/g) || []).length;
    if (pageCount !== panelCount) throw new Error(`PDF page count ${pageCount} != panels ${panelCount}`);
    require('fs').writeFileSync(output, pdf);
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error.message); process.exitCode = 1; });
