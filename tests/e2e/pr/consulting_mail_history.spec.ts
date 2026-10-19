import { test, expect } from '@playwright/test';
import { readFileSync, mkdirSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { findLayoutProblems } from '../helpers/layout_check';
import { consultingFixture } from './helpers/consulting_layout_fixture';
import { PR_BASE_URL } from './helpers/fixture_values';
import { login } from './helpers/login';
import { riskFixture, goodFixture, scopeFixture } from '../../../frontend/src/screens/job-copy/consultantRisk.testFixtures';

// Rustの具体値テストと完全一致する架空のAPI応答。
const template = readFileSync(resolve(__dirname, '../../../templates/tabs/cs_dashboard.html'), 'utf8');
test('案件の詳細にメールの全種類とZoom開催記録を表示する', async ({ page }) => {
  const detail = JSON.parse(readFileSync(resolve(__dirname, '../../fixtures/cs_dashboard/deal-mail-history.json'), 'utf8'));
  await page.setViewportSize({ width: 1440, height: 900 });
  const fixture = consultingFixture(1, 3, false);
  await page.route('**/consulting', r => r.fulfill({ contentType: 'text/html', body: template.replaceAll('{{ user }}', '架空の利用者') }));
  await page.route('**/api/consulting/**', r => {
    const u = new URL(r.request().url());
    return r.fulfill({ json: u.pathname.endsWith('/deal-detail') ? detail : fixture.api(u.pathname, u.searchParams) });
  });
  await page.goto(`${PR_BASE_URL}/consulting#deal/detail?id=a`);
  const timeline = page.locator('#dd-tl');
  await expect(timeline).toBeVisible();
  await page.locator('#dd-older').click();
  await expect(timeline.locator('li.k-email')).toHaveCount(5);
  await expect(timeline.locator('li.k-email .tag')).toHaveText(['応募の報告', 'MTG取り下げ', 'MTG候補', 'MTG予定', 'MTG実施']);
  await expect(timeline.locator('li.k-email').last()).toContainText('2026-03-11');
  await expect(timeline.locator('li.k-email').last()).toContainText('08:00');
  await expect(timeline.locator('li.k-email').last()).toContainText('送った');
  await expect(timeline.locator('li.k-email').last()).toContainText('架空の打ち合わせ報告');
  await expect(timeline.locator('li.k-email').last()).toContainText('Zoom で開催 10:00');
  await expect(timeline.locator('li.k-email').last()).toContainText('架空の継続契約');
  await expect(timeline).toContainText('（Zoom の開催記録・事実）　10:00〜11:00');
  await expect(timeline.locator('li.k-mtg:not(.est)')).toHaveCount(4);
  await expect(timeline.locator('li.est')).toHaveCount(1);
  await expect(timeline).toContainText('契約期間の外（契約前）');
  await expect(timeline).toContainText('契約期間の外（契約の間）');
  await expect(page.locator('.dd-kv > div').nth(0)).toContainText('2026-03-11（20日前）');
  await expect(page.locator('.dd-kv > div').nth(1)).toContainText('2026-03-12（19日前）');
  await expect(page.locator('#cs-main')).not.toContainText('記録が無いのではありません');
  await expect(page.locator('#cs-main .legend').last().locator('.full')).toHaveCount(0);
  await expect(page.locator('#dd-count')).toContainText('メール 5 件');
  for (const kind of ['MTG実施', 'MTG予定', 'MTG候補', 'MTG取り下げ', '応募の報告']) {
    await expect(page.locator('#dd-count')).toContainText(`${kind} 1 件`);
  }
  await expect(page.locator('#cs-main')).not.toContainText('は並べていません');
  await page.locator('#dd-k-email').uncheck();
  await expect(timeline.locator('li.k-email')).toHaveCount(0);
  await expect(timeline.locator('li.k-mtg:not(.est)')).toHaveCount(4);
  await page.locator('#dd-k-email').check();
  await page.locator('#dd-k-mtg').uncheck();
  await expect(timeline.locator('li.k-mtg')).toHaveCount(0);
  await expect(timeline.locator('li.k-email')).toHaveCount(5);
  await page.locator('#dd-k-mtg').check();
  const shots = process.env.CS_MAIL_HISTORY_SHOTS;
  if (shots) {
    mkdirSync(shots, { recursive: true });
    await page.screenshot({ path: join(shots, '1440-900-overview.png') });
    await page.locator('#dd-tl-h').evaluate(el => window.scrollTo(0, el.getBoundingClientRect().top + window.scrollY - 24));
    await page.screenshot({ path: join(shots, '1440-900-mail-history.png') });
    await page.screenshot({ path: join(shots, 'mail-history-full.png'), fullPage: true });
  }
  const height = await page.evaluate(() => document.documentElement.scrollHeight);
  for (let y = 0; y < height; y += 900) {
    await page.evaluate(top => window.scrollTo(0, top), y);
    expect(await findLayoutProblems(page, '.wrap')).toEqual({ overlaps: [], spills: [], svgClipped: [], pageOverflow: false });
  }
});

for (const view of ['risk', 'good']) test(`案件管理の${view === 'risk' ? '危ない案件' : 'いい案件'}にもメールとZoom開催記録が出る`, async ({ page }) => {
  const detail = JSON.parse(readFileSync(resolve(__dirname, '../../fixtures/cs_dashboard/deal-mail-history.json'), 'utf8'));
  const fixture = view === 'risk' ? riskFixture : goodFixture;
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.route('**/api/job-copy/consultant-scope?*', r => r.fulfill({ json: scopeFixture }));
  await page.route(`**/api/job-copy/consultant-${view}?*`, r => r.fulfill({ json: { ...fixture, rows: fixture.rows.slice(0, 1).map(row => ({ ...row, days_since_contact: null, mtg_last: null })) } }));
  await page.route('**/api/consulting/deal-detail?*', r => r.fulfill({ json: detail }));
  await page.route('**/api/job-copy/listings?*', r => r.fulfill({ json: { status: 'ready', total: 0, next_offset: null, listings: [] } }));
  await page.route('**/api/crm/workspace/deals/*?*', r => r.fulfill({ json: { hubspot_portal_id: '', selected: { deal: { notes_last_contacted: '2026-03-30' } }, partial: [] } }));
  await login(page);
  await page.goto(`/app/job-copy?view=${view}&owner=11&owner_field=consultant&risk_deal=101`);
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('heading', { name: 'メール・電話・MTGの記録' })).toBeVisible();
  await expect(dialog).toContainText('接触：2026/03/11 ／ MTG：2026/03/11');
  const email = dialog.locator('article').filter({ has: page.getByRole('heading', { name: / メール / }) });
  await expect(email).toHaveCount(5);
  await expect(email.first()).toContainText('応募の報告');
  await expect(email.last()).toContainText('Zoom で開催 10:00');
  const zoom = dialog.locator('article').filter({ hasText: 'Zoom の開催記録・事実' });
  await expect(zoom).toHaveCount(3);
  await expect(zoom.filter({ hasText: '10:00〜11:00' })).toHaveCount(1);
  await expect(zoom.filter({ hasText: '契約期間の外（契約前）' })).toHaveCount(1);
  await expect(zoom.filter({ hasText: '契約期間の外（契約の間）' })).toHaveCount(1);
  expect(await dialog.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
  if (process.env.CS_MAIL_HISTORY_SHOTS) {
    mkdirSync(process.env.CS_MAIL_HISTORY_SHOTS, { recursive: true });
    await email.first().scrollIntoViewIfNeeded();
    await page.screenshot({ path: join(process.env.CS_MAIL_HISTORY_SHOTS, `1440-900-${view}-history.png`) });
  }
});
