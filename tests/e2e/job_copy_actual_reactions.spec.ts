import { selectJobFeature } from './job-copy-navigation';
/** Real captured 320-application fixture over loopback; no success responses stubbed.
 * Navigation identity remains a synthetic fixture. This does not prove production
 * OIDC authorization or live HubSpot/Drive availability. Originals and screenshots
 * stay in ignored private storage. Assertions never echo applicant text or IDs. */
import { createHash } from 'node:crypto';
import { readFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { test, expect } from '@playwright/test';

type Job = { hubspotListingId: string; body: string; images: { url: string; contentHash: string }[] };
type Reason = { text: string; source_property: string; version_id: null; collected_at: null };
type Result = { listing_id: string; summary: { total: number }; applicant_reasons: { items: Reason[]; total_applicants: number } };
const evidence = resolve('data/job-copy-local/reaction-browser');
const snapshot = resolve(process.env.JOB_COPY_REACTION_MOC_PATH ?? 'data/job-copy-local/applicant-review/reaction-moc-320.json');
let jobs: Job[]; let results: Result[];
test.beforeAll(() => {
  try {
    const raw = JSON.parse(readFileSync(snapshot, 'utf8'));
    jobs = raw.capture_bundle.jobs; results = raw.results;
    if (!Array.isArray(jobs) || !Array.isArray(results)) throw new Error();
  } catch { throw new Error('Required private 320-application snapshot is missing or invalid.'); }
  expect(jobs.length).toBe(36);
  expect(results.reduce((n, row) => n + row.summary.total, 0)).toBe(320);
  expect(results.reduce((n, row) => n + row.applicant_reasons.items.length, 0)).toBe(12);
  expect(jobs.reduce((n, job) => n + job.images.length, 0)).toBe(45);
  expect(results.every(row => row.applicant_reasons.total_applicants === row.summary.total)).toBe(true);
  expect(results.every(row => row.applicant_reasons.items.every(reason => reason.version_id === null && reason.collected_at === null && reason.source_property === 'oubodouki'))).toBe(true);
  mkdirSync(evidence, { recursive: true });
});

test('36 real jobs retain 45 verified image responses and 320 applications after reason enrichment', async ({ page }) => {
  const hashes = new Map(jobs.flatMap(job => job.images.map(image => [image.url, image.contentHash] as const)));
  const verified = new Set<string>();
  page.on('response', response => {
    const url = new URL(response.url()); const key = url.pathname + url.search;
    const hash = hashes.get(key);
    if (hash && response.status() === 200) void response.body().then(bytes => {
      if (createHash('sha256').update(bytes).digest('hex') === hash) verified.add(key);
    }).catch(() => {});
  });
  await page.route('**/*', async route => {
    if (new URL(route.request().url()).origin !== 'http://127.0.0.1:5189' || route.request().method() !== 'GET') {
      await route.abort('blockedbyclient'); throw new Error('Unexpected external or write request.');
    }
    await route.continue();
  });
  await page.goto('/app/job-copy?data=actual');
  await expect(page.locator('.jc-job')).toHaveCount(36);
  await expect(page.locator('.jc-snapshot-summary')).toContainText('320');
  for (let index = 0; index < jobs.length; index++) {
    const job = jobs[index]; if (!job?.images.length) continue;
    await page.locator('.jc-job').nth(index).click();
    await expect.poll(async () => page.locator('.jc-reading img').evaluateAll(images => images.every(image => image instanceof HTMLImageElement && image.complete && image.naturalWidth > 0))).toBe(true);
    await expect(page.locator('.jc-reading img')).toHaveCount(job.images.length);
    expect(await page.locator('.jc-body').textContent() === job.body).toBe(true);
  }
  await expect.poll(() => verified.size).toBe(45);
});

test('actual recorded reasons are unknown-cohort originals with desktop/mobile visibility and print exclusion', async ({ page }) => {
  await page.goto('/app/job-copy?data=actual');
  await expect(page.locator('.jc-job')).toHaveCount(36);
  let observed = 0; let captured = false;
  for (const result of results) {
    if (!result.applicant_reasons.items.length) continue;
    const index = jobs.findIndex(job => job.hubspotListingId === result.listing_id);
    if (index < 0) throw new Error('Reason fixture has an unmatched listing.');
    await page.locator('.jc-job').nth(index).click();
    await selectJobFeature(page, 'reasons');
    const region = page.getByRole('region', { name: '応募理由の記述比較', exact: true });
    const unknown = region.getByRole('region', { name: 'どの版への理由か不明な記述', exact: true });
    await expect(unknown.locator('blockquote')).toHaveCount(result.applicant_reasons.items.length);
    await expect(region.locator('details[open]')).toHaveCount(0);
    await expect(region.getByRole('region', { name: '比較元の記述', exact: true })).toContainText('版との対応は未取得');
    await expect(region.getByRole('region', { name: '比較先の記述', exact: true })).toContainText('版との対応は未取得');
    expect(await unknown.locator('blockquote').allTextContents().then(texts => texts.every((text, i) => text === result.applicant_reasons.items[i]?.text))).toBe(true);
    observed += result.applicant_reasons.items.length;
    if (!captured) {
      captured = true;
      await region.scrollIntoViewIfNeeded();
      await page.screenshot({ path: resolve(evidence, 'reasons-desktop.png') });
      await region.getByText('記録された文を開く（社内確認用）', { exact: true }).first().click();
      await expect(unknown.locator('blockquote').first()).toBeVisible();
      await page.setViewportSize({ width: 375, height: 850 });
      await region.scrollIntoViewIfNeeded();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
      await page.screenshot({ path: resolve(evidence, 'reasons-mobile.png') });
      await selectJobFeature(page, 'report');
      await page.getByRole('region', { name: '顧客報告と検証記録', exact: true }).getByText('記録された文を開く（社内確認用）', { exact: true }).first().click();
      await page.emulateMedia({ media: 'print' });
      for (const original of await page.locator('.ar-reasons').all()) await expect(original).toBeHidden();
      await page.emulateMedia({ media: 'screen' });
      await page.setViewportSize({ width: 1280, height: 900 });
    }
  }
  expect(observed).toBe(12);
});
