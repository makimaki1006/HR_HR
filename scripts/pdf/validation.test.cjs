// Run: node --test scripts/pdf/validation.test.cjs
// On Linux, set PDF_CHROMIUM_PATH to the installed Chromium executable.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { mkdtemp, readFile, writeFile, rm, access } = require('node:fs/promises');
const { tmpdir } = require('node:os');
const { join, resolve } = require('node:path');
const { pathToFileURL } = require('node:url');
const { promisify } = require('node:util');
const execFile = promisify(require('node:child_process').execFile);
const modulePath = process.env.PDF_PLAYWRIGHT_MODULE || resolve('node_modules/playwright-core');
const { chromium } = require(modulePath);
const executable = process.env.PDF_CHROMIUM_PATH || (process.platform === 'win32'
  ? 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' : chromium.executablePath());
const panels = ['excel', 'google', 'indeed', 'population', 'consultation'];
const fixture = `<html><head><style>@page{size:A3 landscape;margin:8mm}
body{margin:0}.pdf-page{position:relative;width:1510px;height:1045px;break-after:page}
.pdf-page:last-child{break-after:auto}.pdf-content{width:1510px}</style></head><body>
${panels.map(id => `<section class="pdf-page"><div class="pdf-content" id="panel-${id}"><p>${id} sentinel</p></div></section>`).join('')}
<script>function fitPages(){};document.documentElement.dataset.pdfReady='true';</script></body></html>`;

test('renderer rejects missing, hidden and overflowing content', async t => {
  const dir = await mkdtemp(join(tmpdir(), 'pdf-reverse-'));
  try {
    for (const [name, html, valid] of [
      ['baseline', fixture, true],
      ['left-overflow', fixture.replace('excel sentinel', '<span style="position:absolute;left:-500px">lost</span>excel sentinel'), false],
      ['right-overflow', fixture.replace('excel sentinel', '<span style="position:absolute;left:1600px">lost</span>excel sentinel'), false],
      ['bottom-overflow', fixture.replace('excel sentinel', '<span style="position:absolute;top:1100px">lost</span>excel sentinel'), false],
      ['missing-panel', fixture.replace('class="pdf-content" id="panel-google"', 'id="panel-google"'), false],
      ['hidden-panel', fixture.replace('id="panel-google"', 'id="panel-google" style="display:none"'), false],
      ['wrong-order', fixture.replace('id="panel-google"', 'id="panel-unexpected"'), false],
    ]) {
      await t.test(name, async () => {
        const input = join(dir, `${name}.html`), output = join(dir, `${name}.pdf`);
        await writeFile(input, html);
        const run = execFile(process.execPath, [join(__dirname, 'render.cjs'), executable,
          pathToFileURL(input).href, output, modulePath], { timeout: 25000 });
        if (valid) {
          await run;
          const bytes = await readFile(output);
          assert.equal(bytes.subarray(0, 5).toString(), '%PDF-');
        } else {
          await assert.rejects(run, error => error.stderr.includes('PDF page fitting failed'));
          await assert.rejects(access(output));
        }
      });
    }
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test('download form rejects failed or incomplete responses and retains inputs', async t => {
  const browser = await chromium.launch({ executablePath: executable, headless: true });
  try {
    const html = await readFile(resolve('templates/competitor.html'), 'utf8');
    for (const [name, status, mime, body] of [
      ['503', 503, 'text/plain', 'generation failed'],
      ['wrong-mime', 200, 'text/html', '<html>login</html>'],
      ['empty-pdf', 200, 'application/pdf', ''],
      ['html-as-pdf', 200, 'application/pdf', '<html>error</html>'],
      ['truncated-pdf', 200, 'application/pdf', '%PDF-1.7\ntruncated'],
      ['network-error', 0, '', ''],
      ['expired-login', 302, '', ''],
    ]) {
      await t.test(name, async () => {
        const page = await browser.newPage();
        try {
          let downloads = 0;
          page.on('download', () => downloads++);
          await page.route('http://audit.test/competitor', r => r.fulfill({ body: html, contentType: 'text/html' }));
          await page.route('http://audit.test/login', r => r.fulfill({ body: 'login', contentType: 'text/html' }));
          await page.route('http://audit.test/report/competitor', r => status === 0 ? r.abort()
            : r.fulfill({ status, contentType: mime || 'text/plain', body,
              headers: status === 302 ? { location: '/login' } : {} }));
          await page.goto('http://audit.test/competitor');
          await page.locator('#survey-title').fill('retained condition');
          await page.locator('#csv-file').setInputFiles({ name: 'test.csv', mimeType: 'text/csv', buffer: Buffer.from('test') });
          await page.locator('button[value=pdf]').click();
          await page.waitForFunction(() => document.querySelector('#submit-status').getAttribute('role') === 'alert');
          assert.equal(downloads, 0);
          assert.equal(page.url(), 'http://audit.test/competitor');
          assert.equal(await page.locator('#survey-title').inputValue(), 'retained condition');
          assert.equal(await page.locator('button[value=pdf]').isEnabled(), true);
          assert.equal(await page.locator('button[value=html]').isEnabled(), true);
        } finally { await page.close(); }
      });
    }
    await t.test('complete-pdf-download', async () => {
      const page = await browser.newPage({ acceptDownloads: true });
      try {
        await page.setContent(fixture);
        const pdf = await page.pdf({ preferCSSPageSize: true });
        await page.route('http://audit.test/competitor', r => r.fulfill({ body: html, contentType: 'text/html' }));
        await page.route('http://audit.test/report/competitor', r => r.fulfill({ body: pdf, contentType: 'application/pdf' }));
        await page.goto('http://audit.test/competitor');
        await page.locator('#csv-file').setInputFiles({ name: 'test.csv', mimeType: 'text/csv', buffer: Buffer.from('test') });
        const event = page.waitForEvent('download');
        await page.locator('button[value=pdf]').click();
        const download = await event;
        assert.equal(await download.failure(), null);
        assert.deepEqual(await readFile(await download.path()), pdf);
        assert.equal(page.url(), 'http://audit.test/competitor');
      } finally { await page.close(); }
    });
  } finally { await browser.close(); }
});
