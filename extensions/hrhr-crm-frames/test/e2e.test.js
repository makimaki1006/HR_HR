// 実ブラウザ E2E。拡張を読み込んだ Chromium で、フレーム拒否サイトが CRM タブ内だけで表示されることを確かめる。
// 実行: (リポジトリ直下で npm ci 済み) cd extensions/hrhr-crm-frames && npm run test:e2e
import test, { before, after, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';

const EXT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const VERSION = JSON.parse(fs.readFileSync(path.join(EXT, 'manifest.json'), 'utf8')).version;
const APP_PORT = 9216; // 拡張の既定オリジン http://127.0.0.1:9216
const SITE_PORT = 9301;

const listen = (srv, port) => new Promise((res, rej) => { srv.once('error', rej); srv.listen(port, '127.0.0.1', res); });

// フレームされたページ: 読み込めたら親へ通知する (クロスオリジンでも postMessage は届く)。
const SITE_PAGE = (host) => `<!doctype html><title>${host}</title>
<script>parent.postMessage('loaded:' + location.host, '*')</script>
<a id="next" href="http://second-ok.test:${SITE_PORT}/">next</a>`;

const siteServer = http.createServer((req, res) => {
  const host = (req.headers.host || '').split(':')[0];
  if (host === 'plain.test') { // CRM ではない普通のページ (フレーム拒否サイトを埋め込む)
    res.writeHead(200, { 'content-type': 'text/html' });
    res.end(`<iframe id="f" src="http://frame-ok.test:${SITE_PORT}/"></iframe>
<script>window.__msgs=[];addEventListener('message',e=>__msgs.push(e.data))</script>`);
    return;
  }
  res.writeHead(200, {
    'content-type': 'text/html',
    'x-frame-options': 'DENY',
    'content-security-policy': "frame-ancestors 'none'",
  });
  res.end(SITE_PAGE(host));
});

const appServer = http.createServer((req, res) => {
  const u = new URL(req.url, 'http://x');
  if (u.pathname === '/app/crm' || u.pathname === '/app/other') {
    res.writeHead(200, { 'content-type': 'text/html' });
    res.end(`<iframe id="f" src="${u.searchParams.get('src') || 'about:blank'}"></iframe>
<script>window.__msgs=[];addEventListener('message',e=>__msgs.push(e.data))</script>`);
    return;
  }
  res.writeHead(404); res.end();
});

let ctx, sw, userDir;

before(async () => {
  await listen(appServer, APP_PORT);
  await listen(siteServer, SITE_PORT);
  userDir = fs.mkdtempSync(path.join(os.tmpdir(), 'hrhr-ext-'));
  ctx = await chromium.launchPersistentContext(userDir, {
    channel: 'chromium', // 新ヘッドレス (拡張が使える)。headless-shell では拡張が動かない
    headless: true,
    args: [
      `--disable-extensions-except=${EXT}`,
      `--load-extension=${EXT}`,
      '--host-resolver-rules=MAP *.test 127.0.0.1',
    ],
  });
  sw = ctx.serviceWorkers()[0] ?? (await ctx.waitForEvent('serviceworker'));
  // ブロックリスト: blocked.test だけ (sync ストレージに保存 → onChanged で再構築される)
  await sw.evaluate(() => chrome.storage.sync.set({ blocklist: ['blocked.test'] }));
});

after(async () => {
  await ctx?.close();
  appServer.close();
  siteServer.close();
  fs.rmSync(userDir, { recursive: true, force: true });
});

const ruleCount = () => sw.evaluate(async () => (await chrome.declarativeNetRequest.getSessionRules()).length);
async function waitFor(fn, msg, ms = 10_000) {
  const end = Date.now() + ms;
  for (;;) {
    const v = await fn();
    if (v) return v;
    if (Date.now() > end) assert.fail('timeout: ' + msg);
    await new Promise((r) => setTimeout(r, 100));
  }
}
const msgs = (page) => page.evaluate(() => window.__msgs);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const APP = `http://127.0.0.1:${APP_PORT}`;
const site = (host) => `http://${host}:${SITE_PORT}/`;
const setSrc = (page, src) => page.evaluate((s) => { document.getElementById('f').src = s; }, src);
const subFrame = (page) => page.frames().find((f) => f !== page.mainFrame());
// 拒否されたフレームは http(s) の URL を持たない (chrome-error:// か空)。
const isRefused = (page) => !/^https?:/.test(subFrame(page)?.url() ?? '');

// CRM タブを開き、ルールが入るのを待つ (利用者が検索してから iframe を開く順序に合わせる)。
async function openCrm() {
  const page = await ctx.newPage();
  await page.goto(`${APP}/app/crm`);
  await waitFor(async () => (await ruleCount()) >= 1, 'rule exists');
  return page;
}

// 失敗しても残タブが次のテストへ漏れないよう、毎回すべてのタブを閉じる (ブラウザ維持用に 1 枚だけ残す)。
let keeper;
afterEach(async () => {
  for (const p of ctx.pages()) if (p !== keeper) await p.close().catch(() => {});
  await waitFor(async () => (await ruleCount()) === 0, 'rules cleared after closing tabs', 5000);
});

test('0. CRM タブが無ければルールは無い / content script が版を出す', async () => {
  keeper = await ctx.newPage();
  assert.equal(await ruleCount(), 0);
  const page = await openCrm();
  assert.equal(await page.evaluate(() => document.documentElement.dataset.hrhrFrames), VERSION);
});

test('1. CRM タブ内ではフレーム拒否サイトが iframe に表示される (iframe 内リンク遷移も)', async () => {
  const page = await openCrm();
  await setSrc(page, site('frame-ok.test'));
  await waitFor(async () => (await msgs(page)).includes(`loaded:frame-ok.test:${SITE_PORT}`), 'framed site loaded');
  const frame = subFrame(page);
  assert.equal(await frame.title(), 'frame-ok.test');
  // iframe 内のリンクをクリック (遷移の initiator は拡張が知らないサイト自身)
  await frame.click('#next');
  await waitFor(async () => (await msgs(page)).includes(`loaded:second-ok.test:${SITE_PORT}`), 'link navigation inside iframe');
});

test('2. ブロックリストのドメインは CRM タブ内でも拒否のまま', async () => {
  const page = await openCrm();
  await setSrc(page, site('blocked.test'));
  await sleep(1500);
  assert.deepEqual(await msgs(page), []);
  assert.ok(isRefused(page));
});

test('3. CRM 以外のタブ (普通のページ) では拒否のまま。CRM タブが同時にあっても影響しない', async () => {
  await openCrm();
  const other = await ctx.newPage();
  await other.goto(site('plain.test'));
  await sleep(1500);
  assert.deepEqual(await msgs(other), []);
  assert.ok(isRefused(other));
});

test('4. CRM タブが /app/crm から離れるとルールが消え、再び拒否される', async () => {
  const page = await openCrm();
  await page.goto(`${APP}/app/other`);
  await waitFor(async () => (await ruleCount()) === 0, 'rule removed after leaving /app/crm');
  await setSrc(page, site('frame-ok.test'));
  await sleep(1500);
  assert.deepEqual(await msgs(page), []);
  assert.ok(isRefused(page));
  // content script はアプリのオリジン全体に入る (契約)
  assert.equal(await page.evaluate(() => document.documentElement.dataset.hrhrFrames), VERSION);
});

test('5. ルールの中身: tabIds は CRM タブのみ、sub_frame のみ、除外に blocked.test とアプリのホスト', async () => {
  await openCrm();
  await ctx.newPage(); // CRM ではないタブ
  const [rule] = await sw.evaluate(() => chrome.declarativeNetRequest.getSessionRules());
  const c = rule.condition;
  assert.equal(c.tabIds.length, 1);
  assert.deepEqual(c.resourceTypes, ['sub_frame']);
  assert.ok(c.excludedRequestDomains.includes('blocked.test'));
  assert.ok(c.excludedRequestDomains.includes('127.0.0.1'));
});
