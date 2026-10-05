import { selectJobFeature } from './job-copy-navigation';
/** Frontend fixture E2E: candidate shared AppShell uses synthetic /api/nav.
 * Real previously captured data/images use local HTTP without success stubs.
 * This does not demonstrate production OIDC authorization, continuous Drive availability or live writes.
 * Private customer text/IDs/URLs never enter assertions or public artifacts. */
import { createHash } from 'node:crypto';
import { readFileSync, mkdirSync } from 'node:fs';
import { resolve, relative, isAbsolute } from 'node:path';
import { test, expect, type Page } from '@playwright/test';

type Evidence = { url: string; sha256: string };
type Target = { index: number; images: Evidence[] };
type Job = { hubspotListingId: string; company: string; body: string; images: { url: string; contentHash: string }[] };
type Aggregate = { listing_id: string; summary: { total: number }; dated_comparison: { unknown: number } };
const integration = resolve('../gurnard/data/job-copy-local/applicant-review/expansion/integration');
const visuals = resolve('data/job-copy-local/candidate-browser/visual');
const sha = (bytes: Buffer) => createHash('sha256').update(bytes).digest('hex');
let targets: Target[];
let jobs: Job[];
let aggregates: Aggregate[];
let evidenceByUrl: Map<string, Evidence>;

test.beforeAll(() => {
  try {
    const moc = JSON.parse(readFileSync(resolve(integration, 'drive-moc.json'), 'utf8'));
    const linked = JSON.parse(readFileSync(resolve(integration, 'link-results.json'), 'utf8')).results;
    if (moc.schemaVersion !== 1 || !Array.isArray(moc.capture_bundle?.jobs) || !Array.isArray(linked)) throw new Error();
    jobs = moc.capture_bundle.jobs;
    aggregates = moc.results;
    evidenceByUrl = new Map();
    targets = linked.map((row: { listing_id: string; images: { url: string; path: string; mime: string; sha256: string; bytes: number }[] }) => {
      const index = jobs.findIndex(job => job.hubspotListingId === row.listing_id);
      if (index < 0 || !Array.isArray(row.images) || !row.images.length) throw new Error();
      const images = row.images.map(image => {
        if (!/^\/api\/job-copy\/image\?company_id=\d{1,30}&listing_id=\d{1,30}&manifest_id=[A-Za-z0-9_-]{10,200}&slot=[1-9]\d*$/.test(image.url)
          || !['image/jpeg', 'image/png', 'image/webp'].includes(image.mime) || !/^[a-f0-9]{64}$/.test(image.sha256)) throw new Error();
        const path = resolve('../gurnard', image.path);
        const boundary = relative(integration, path);
        if (!boundary || boundary.startsWith('..') || isAbsolute(boundary)) throw new Error();
        const bytes = readFileSync(path);
        if (bytes.length !== image.bytes || sha(bytes) !== image.sha256 || evidenceByUrl.has(image.url)) throw new Error();
        if (!jobs[index].images.some(entry => entry.url === image.url && entry.contentHash === image.sha256)) throw new Error();
        const evidence = { url: image.url, sha256: image.sha256 };
        evidenceByUrl.set(image.url, evidence);
        return evidence;
      });
      return { index, images };
    });
  } catch {
    throw new Error('Required private expansion evidence is missing or inconsistent; complete the Rust integration before running.');
  }
  expect(jobs.length).toBe(36);
  expect(targets.length).toBe(28);
  expect(new Set(targets.map(target => target.index)).size).toBe(28);
  expect(evidenceByUrl.size).toBe(45);
  expect(new Set([...evidenceByUrl.values()].map(row => row.sha256)).size).toBe(34);
  expect(aggregates.reduce((sum, row) => sum + row.summary.total, 0)).toBe(317);
  expect(aggregates.reduce((sum, row) => sum + row.dated_comparison.unknown, 0)).toBe(314);
  mkdirSync(visuals, { recursive: true });
});

async function open(page: Page) {
  const verified = new Set<string>();
  page.on('response', response => {
    const url = new URL(response.url());
    const key = url.pathname + url.search;
    const evidence = evidenceByUrl.get(key);
    if (evidence && response.status() === 200) void response.body().then(bytes => {
      if (sha(bytes) === evidence.sha256) verified.add(key);
    }).catch(() => {});
  });
  // Continue success requests unchanged; prohibit external or write requests.
  await page.route('**/*', async route => {
    if (new URL(route.request().url()).origin !== 'http://127.0.0.1:5188' || route.request().method() !== 'GET') {
      await route.abort('blockedbyclient');
      throw new Error('Unexpected non-local or write request in expansion UI verification.');
    }
    await route.continue();
  });
  await page.goto('/app/job-copy?data=actual');
  await expect(page.locator('.jc-job')).toHaveCount(36);
  return verified;
}

test('all 28 linked jobs render 45 proxy references matching 34 unique original byte hashes', async ({ page }) => {
  const verified = await open(page);
  for (const target of targets) {
    await page.locator('.jc-job').nth(target.index).click();
    const gallery = page.getByRole('region', { name: 'この版の掲載画像', exact: true });
    await expect(gallery.getByRole('img')).toHaveCount(target.images.length);
    for (const image of target.images) {
      const imageIndex = await gallery.getByRole('img').evaluateAll((nodes, expected) => nodes.findIndex(node => node.getAttribute('src') === expected), image.url);
      expect(imageIndex >= 0).toBe(true);
      const rendered = gallery.getByRole('img').nth(imageIndex);
      await rendered.scrollIntoViewIfNeeded();
      await expect.poll(() => rendered.evaluate(node => (node as HTMLImageElement).complete && (node as HTMLImageElement).naturalWidth > 0)).toBe(true);
      await expect.poll(() => verified.has(image.url)).toBe(true);
    }
  }
  expect(verified.size).toBe(45);
});

test('candidate shared navigation, exact captured full body, company filter and comparison tabs integrate without geography requests', async ({ page }) => {
  const requested: string[] = [];
  page.on('request', request => { requested.push(new URL(request.url()).pathname); });
  await open(page);
  await expect(page.locator('.hr-header')).toHaveCount(1);
  await expect(page.locator('.jc-topbar')).toHaveCount(0);
  const nav = page.getByRole('navigation', { name: 'ダッシュボードナビ' });
  await expect(nav.getByRole('link', { name: '求人文面', exact: true })).toHaveAttribute('aria-current', 'page');
  expect(await page.locator('.jc-body').first().textContent() === jobs[0].body).toBe(true);
  expect(await page.locator('.jc-app').evaluate(node => getComputedStyle(node).color)).toBe('rgb(37, 57, 76)');
  const company = jobs[0].company;
  await page.getByLabel('取引先', { exact: true }).evaluate((node, value) => {
    (node as HTMLSelectElement).value = value;
    node.dispatchEvent(new Event('change', { bubbles: true }));
  }, company);
  const filtered = jobs.filter(job => job.company === company);
  await expect(page.locator('.jc-job')).toHaveCount(filtered.length);
  await page.locator('.jc-job').last().click();
  expect(await page.locator('.jc-body').first().textContent() === filtered.at(-1)!.body).toBe(true);
  await selectJobFeature(page, 'diff');
  await expect(page.locator('.jc-comparison-overview')).toContainText('変更なし');
  await selectJobFeature(page, 'applicants');
  await expect(page.getByRole('region', { name: '求人全体の実応募者構成' })).toBeVisible();
  await selectJobFeature(page, 'receive');
  await expect(page.getByLabel('受け取った文面')).toBeVisible();
  expect(requested.some(path => path.startsWith('/api/filters/') || path.startsWith('/api/set_'))).toBe(false);
});

test('mobile shared shell stays within the viewport and printed reports hide navigation while preserving long review text', async ({ page }) => {
  await open(page);
  await page.setViewportSize({ width: 375, height: 812 });
  const largest = aggregates.find(row => row.summary.total === 34)!;
  const richIndex = jobs.findIndex(job => job.hubspotListingId === largest.listing_id);
  expect(richIndex >= 0).toBe(true);
  await page.locator('.jc-job').nth(richIndex).click();
  expect(await page.locator('.jc-body').first().textContent() === jobs[richIndex].body).toBe(true);
  await selectJobFeature(page, 'report');
  const hypothesis = '合成の検証記録です。\n'.repeat(40);
  await page.getByRole('textbox', { name: '仮説', exact: true }).fill(hypothesis);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: resolve(visuals, 'report-375.png'), fullPage: true });
  await page.setViewportSize({ width: 794, height: 1123 });
  await page.emulateMedia({ media: 'print' });
  await expect(page.locator('.hr-header')).toBeHidden();
  await expect(page.locator('.hr-nav')).toBeHidden();
  await expect(page.locator('.jc-page-heading')).toBeHidden();
  await expect(page.locator('.jc-snapshot-summary')).toBeHidden();
  await expect(page.locator('.jc-live-panel')).toBeHidden();
  const overall = page.getByRole('region', { name: '求人全体の実応募者構成' });
  await expect(overall).toBeVisible();
  await expect(overall).toContainText('応募34件');
  await expect(overall).toContainText(`版の対応不明${largest.dated_comparison.unknown}件`);
  await expect(overall).toContainText('各版へ割り当てていません');
  await expect(page.getByRole('textbox', { name: '仮説', exact: true })).toBeHidden();
  const printed = page.locator('.jc-print-text').first();
  await expect(printed).toBeVisible();
  expect(await printed.textContent() === hypothesis).toBe(true);
  await page.screenshot({ path: resolve(visuals, 'report-print.png'), fullPage: true });
});

test('expanded image data retains real application totals and the richest job displays images and charts at both widths', async ({ page }) => {
  await open(page);
  const summary = page.getByRole('region', { name: '実データの取得範囲' });
  await expect(summary).toContainText('317応募レコード');
  await expect(summary).toContainText('版対応不明 314件');
  const max = Math.max(...aggregates.map(row => row.summary.total));
  expect(max).toBe(34);
  const richest = aggregates.find(row => row.summary.total === max)!;
  const index = jobs.findIndex(job => job.hubspotListingId === richest.listing_id);
  expect(index >= 0).toBe(true);
  expect(targets.some(target => target.index === index)).toBe(true);
  for (const width of [1440, 375]) {
    await page.setViewportSize({ width, height: width === 375 ? 812 : 1100 });
    await page.locator('.jc-job').nth(index).click();
    await selectJobFeature(page, 'body');
    const image = page.getByRole('region', { name: 'この版の掲載画像', exact: true }).getByRole('img').first();
    await image.scrollIntoViewIfNeeded();
    await expect.poll(() => image.evaluate(node => (node as HTMLImageElement).naturalWidth)).toBeGreaterThan(0);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({ path: resolve(visuals, `images-${width}.png`) });
    await selectJobFeature(page, 'applicants');
    const overall = page.getByRole('region', { name: '求人全体の実応募者構成' });
    await expect(overall).toContainText('応募34件');
    await expect(overall.locator('.ac-chart')).toHaveCount(4);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({ path: resolve(visuals, `applicants-${width}.png`), fullPage: true });
  }
});
