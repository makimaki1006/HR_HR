import { test, expect, type Page } from '@playwright/test';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { login, getChartSeriesLengths } from './helpers/login';
import { findListings } from '../job-copy-navigation';
import { findLayoutProblems } from '../helpers/layout_check';
import { demoHome } from '../../../frontend/src/screens/job-copy/demoHome';
import { demoOwnerTodo } from '../../../frontend/src/screens/job-copy/demoOwnerTodo';
import { boardFixture, boardRows } from '../../../frontend/src/screens/job-copy/dealBoard.testFixtures';
import { contractCatalog, contractSchema, contractDetail, contractValues } from '../../../frontend/src/screens/job-copy/dealContract.testFixtures';
import { fixtureJobs, fixtureHistory } from '../job-copy-listings-fixture.mjs';

const long = '架空の法人と拠点と担当者の長い名前'.repeat(3) + 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789'.repeat(2);
const emptyReport = { overlaps: [], spills: [], svgClipped: [], pageOverflow: false };
const cases = [
  { name: 'empty', count: 0, weeks: 1, text: '', value: null },
  { name: 'one-week', count: 1, weeks: 1, text: long, value: 0 },
  { name: 'one-month', count: 8, weeks: 4, text: '仮', value: -123456789 },
  { name: 'two-years', count: 80, weeks: 110, text: long, value: 987654321 },
];
async function audit(page: Page, label: string) {
  await page.mouse.move(1200, 90);
  await page.waitForTimeout(250); // サイドバーの開閉と画面遷移のアニメーションが終わってから測る。
  const root = await page.locator('dialog[open]').count() ? 'dialog[open]' : await page.locator('.mo-drawer').count() ? '.mo-drawer' : '.jc-shell';
  // details は実際の操作で開く。横スクロール枠は左右端も検査する。
  const summaries = page.locator(root).locator('details:visible:not(.jc-infotip):not([open]) > summary');
  while (await summaries.count()) await summaries.first().click({ timeout: 5000 });
  await page.evaluate(() => document.fonts.ready);
  for (const position of [0, 0.5, 1]) {
    await page.locator(root).evaluate((element, fraction) => {
      for (const node of element.querySelectorAll<HTMLElement>('*')) {
        if (!node.checkVisibility()) continue;
        const style = getComputedStyle(node);
        if (['auto', 'scroll'].includes(style.overflowY)) node.scrollTop = (node.scrollHeight - node.clientHeight) * fraction;
        if (['auto', 'scroll'].includes(style.overflowX)) node.scrollLeft = (node.scrollWidth - node.clientWidth) * fraction;
      }
    }, position);
    const report = await findLayoutProblems(page, root);
    const name = `${label}-${position}`;
    const dir = process.env.JOBCOPY_LAYOUT_SHOTS;
    if (dir) {
      mkdirSync(dir, { recursive: true });
      await page.screenshot({ path: join(dir, `${name}.png`), clip: { x: 0, y: 100, width: page.viewportSize()!.width, height: page.viewportSize()!.height - 100 } });
      writeFileSync(join(dir, `${name}.json`), JSON.stringify(report, null, 2));
    }
    expect.soft(report, name).toEqual(emptyReport);
  }
  // 長い節の途中にある図も、下端まで見える位置に移して検査する。
  for (const [index, target] of (await page.locator(root).locator('.lf:visible, .jt-lane:visible').all()).entries()) {
    await target.evaluate(element => element.scrollIntoView({ block: 'end', inline: 'nearest' }));
    for (const edge of [0, 1]) {
      await target.locator('.lf-scroll').evaluateAll((elements, fraction) => { for (const element of elements) element.scrollLeft = (element.scrollWidth - element.clientWidth) * fraction; }, edge);
      const report = await findLayoutProblems(page, root);
      const name = `${label}-chart-${index}-${edge}`;
      const dir = process.env.JOBCOPY_LAYOUT_SHOTS;
      if (dir) {
        mkdirSync(dir, { recursive: true });
        await page.screenshot({ path: join(dir, `${name}.png`), clip: { x: 0, y: 100, width: page.viewportSize()!.width, height: page.viewportSize()!.height - 100 } });
        writeFileSync(join(dir, `${name}.json`), JSON.stringify(report, null, 2));
      }
      expect.soft(report, name).toEqual(emptyReport);
    }
  }
  // 説明の吹き出しは背景を覆うため、中の文字を別に測る。
  for (const [index, summary] of (await page.locator(root).locator('.jc-infotip:visible > summary').all()).entries()) {
    const reachable = await summary.evaluate(element => {
      const r = element.getBoundingClientRect();
      const front = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
      return front !== null && (element.contains(front) || front.contains(element));
    });
    if (!reachable) continue;
    await summary.click({ timeout: 5000 });
    const tip = '.jc-infotip[open] .jc-infotip-body';
    await expect(page.locator(tip)).toBeVisible();
    const report = await findLayoutProblems(page, tip);
    expect.soft(report, `${label}-tip-${index}`).toEqual(emptyReport);
    await summary.click({ timeout: 5000 });
  }
}

for (const width of [1280, 1440]) for (const scenario of cases) test(`案件管理の文字配置 ${width} ${scenario.name}`, async ({ page }) => {
  test.setTimeout(180_000);
  await page.setViewportSize({ width, height: 1000 });
  await page.clock.setFixedTime(new Date('2026-10-18T03:00:00Z'));
  let ended = false;
  const rows = Array.from({ length: scenario.count }, (_, i) => ({ ...boardRows[i % boardRows.length], id: String(100 + i), name: scenario.text || '仮', company: scenario.text, consultant: scenario.text, amount: scenario.value, cpa: scenario.value, last_mtg: null,
    reasons: Array.from({ length: 6 }, (_, j) => ({ kind: ['expiration', 'contact', 'stalled', 'nps', 'target', 'cpa'][j], text: `架空の注意${j + 1}` })) }));
  const listings = Array.from({ length: scenario.count }, (_, i) => ({ ...fixtureJobs[i % fixtureJobs.length], id: String(i + 1), indeed_title_state: scenario.weeks === 1 || i % 2 ? 'unset' : 'set', indeed_title: scenario.text || '仮', media_job_id: String(10000000 + i), title: scenario.text || '仮', application_count: scenario.value, deal_ids: ['100'], listing_started_on: scenario.weeks === 1 ? '2026-10-12' : '2024-09-09', publication_status: '掲載終了' }));
  const work = { ...demoOwnerTodo, owners: demoOwnerTodo.owners.map(o => ({ ...o, name: scenario.text || '仮' })), actions: Object.fromEntries(Object.entries(demoOwnerTodo.actions).map(([k, v]) => [k, rows.map(r => ({ ...v[0], ...r, due: '2026-10-12', content: scenario.text }))])), expirations: [], signals: [] };
  await page.route('**/api/crm/**', route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/property-catalog')) return route.fulfill({ json: contractCatalog });
    if (path.endsWith('/edit-schema')) return route.fulfill({ json: { ...contractSchema, deal_id: '100' } });
    if (path.includes('/workspace/deals/')) {
      const detail = contractDetail({ ...contractValues, amount: String(scenario.value ?? ''), contract_period: scenario.weeks > 4 ? '30' : '1' });
      return route.fulfill({ json: { ...detail, deal: { ...detail.deal, id: '100' } } });
    }
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  await page.route('**/api/job-copy/**', route => {
    const path = new URL(route.request().url()).pathname;
    const total = scenario.weeks === 1 ? 0 : scenario.count;
    const sources = ['oubodouki', 'ouboriyuu_baitaikisai', 'ouboriyuu_hiaringu'];
    const hearing = { total_applicants: total, truncated: false, items: Array.from({ length: total }, (_, i) => ({ applicant: i.toString(16).padStart(64, '0'), application_date: '2026-10-13', media: 'HRハッカー', multi_listing: false, fields: [{ label: '勤務の希望', text: scenario.text || '仮' }], notes: [] })) };
    if (path.endsWith('/phone-hearing')) return route.fulfill({ json: hearing });
    if (path.endsWith('/applicants')) return route.fulfill({ json: {
      listing_id: path.split('/').at(-2), total, missing_date: 0, by_date: total ? { '2026-10-13': total } : {}, fetched_at: '2026-10-18T00:00:00Z',
      applications: hearing.items.map(i => ({ key: i.applicant, date: i.application_date, gender: null, age: null, prefecture: null, municipality: scenario.text })), phone_hearing: hearing,
      applicant_reasons: { available: true, source: 'hubspot', basis: 'recorded_applicant_reason', source_property: null, fetched_at: '2026-10-18T00:00:00Z', total_applicants: total, total_source_values: total * 3, missing: total * 2, blank: 0, truncated: false,
        source_counts: Object.fromEntries(sources.map((key, i) => [key, { missing: i ? total : 0, blank: 0, nonblank: i ? 0 : total }])),
        items: hearing.items.map((i, index) => ({ id: index.toString(16).padStart(64, '0'), text: scenario.text || '仮', source: 'hubspot', source_property: 'oubodouki', application_date: i.application_date, collected_at: null, version_id: null })),
      },
    } });
    if (path.endsWith('/market')) {
      const months = Array.from({ length: scenario.weeks > 4 ? 30 : 1 }, (_, i) => new Date(Date.UTC(2024, 4 + i, 1)).toISOString().slice(0, 7));
      return route.fulfill({ json: { source: '架空の市場', titles: [scenario.text || '仮', ...Array.from({ length: Math.max(0, scenario.count - 1) }, (_, i) => `${scenario.text || '仮'}${i + 1}`)], prefectures: ['大分県'], series: scenario.weeks === 1 ? null : { prefecture: '大分県', months, job_count: months.map(() => Math.abs(scenario.value ?? 0)), ctk_count: months.map(() => Math.abs(scenario.value ?? 0)), employer_count: months.map(() => 0), seekers_per_posting: months.map(() => null) } } });
    }
    if (path.endsWith('/company-deals')) return route.fulfill({ json: { company: { id: '900', name: scenario.text || '仮' }, deals: rows.slice(0, 6).map(r => ({ ...r, name: scenario.text || '仮', stage_label: '契約中', current: false, listings_live: 0, listings_total: listings.length, contract_start: '2024-09-09', contract_end: '2026-10-18' })), index_ready: true } });
    if (path.endsWith('/deal-applications')) return route.fulfill({ json: { shared_applications: 0, company: { id: '900', name: scenario.text || '仮' }, listings: listings.map(l => ({ listing_id: l.id, total: 0, by_date: {}, fetched_at: '2026-10-18T00:00:00Z' })) } });
    if (path.endsWith('/listing-body-dates')) return route.fulfill({ json: { dates: {} } });
    if (path.endsWith('/listings/companies')) return route.fulfill({ json: { companies: Object.fromEntries(listings.map(l => [l.id, scenario.text || '仮'])) } });
    if (path.endsWith('/listing-comparison')) return route.fulfill({ json: { jobs: listings.slice(0, 5).map(listing => ({ listing, body: `仕事内容：${scenario.text}` })) } });
    if (path.endsWith('/home')) return route.fulfill({ json: { ...demoHome, work, home: { ...demoHome.home, customers: listings.map((l, i) => ({ ...demoHome.home.customers[0], company_id: String(900 + i), name: scenario.text || '仮', recent_cost: scenario.value, cost_per_app: scenario.value })), cards: demoHome.home.cards.map(c => ({ ...c, count: scenario.count, listings: listings.map(l => ({ ...c.listings[0], ...l, company: scenario.text, prev_apps: scenario.value, recent_apps: scenario.value, recent_cost: scenario.value })) })) } } });
    if (path.endsWith('/deals')) return route.fulfill({ json: { ...boardFixture, rows, work } });
    if (path.endsWith('/listings')) return route.fulfill({ json: { status: 'ready', listings, total: listings.length, titles: Array.from({ length: scenario.count }, (_, i) => `${scenario.text || '仮'}${i}`), deals: [{ id: '100', name: scenario.text || '仮', count: listings.length }], offset: 0, next_offset: null, index_built_at: '2026-10-18T00:00:00Z', refreshing: false, refresh_failed: false } });
    if (path.endsWith('/versions')) {
      const listing = listings.find(l => l.id === path.split('/').at(-2)) ?? listings[0];
      const history = fixtureHistory('1');
      const versions = Array.from({ length: scenario.weeks }, (_, i) => ({ written_at: new Date(Date.UTC(2026, 9, 12) - (scenario.weeks - i - 1) * 7 * 86400000).toISOString(), body: `仕事内容：${scenario.text}\n給与補足：${i}円`, image_urls: [] }));
      return route.fulfill({ json: { ...history, listing: { ...listing, publication_status: ended ? '掲載終了' : '掲載中' }, status_history: { kyuujin_status: [{ timestamp: versions[0].written_at, value: '掲載中' }, ...(ended ? [{ timestamp: '2026-10-18T00:00:00Z', value: '掲載終了' }] : [])] }, can_write_drafts: true, draft_revision: 'synthetic-revision', current_row: { 案件名: scenario.text, 仕事内容: scenario.text }, company_name: scenario.text, versions, current_fields: { 案件名: scenario.text, 仕事内容: scenario.text }, listing_started_on: versions[0].written_at.slice(0, 10), listing_ended_on: '2026-10-18', indeed_weekly_perf: scenario.weeks > 4 ? { schema_version: 1, source: 'hrhacker', job_id: listing.media_job_id, captured_at: '2026-10-18T00:00:00Z', rows: versions.map(v => ({ period_start: v.written_at.slice(0, 10), period_end: new Date(Date.parse(v.written_at) + 6 * 86400000).toISOString().slice(0, 10), impressions: scenario.value, clicks: scenario.value, applications: scenario.value, cost_yen: scenario.value })) } : null } });
    }
    if (path.endsWith('/recent-changes')) return route.fulfill({ json: { status: 'ready', report: { count: 0, listings: [] } } });
    if (path.endsWith('/moc')) return route.fulfill({ json: { schemaVersion: 1, capture_bundle: { schemaVersion: 1, jobs: [] }, results: [] } });
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  await login(page);
  const prefix = `${width}-${scenario.name}`;
  await page.goto('/app/job-copy?owner=11');
  await expect(page.getByRole('heading', { name: 'ホーム', exact: true })).toBeVisible();
  await audit(page, `${prefix}-home`);
  const cardCount = await page.locator('.jh-card').count();
  for (let index = 0; index < cardCount; index++) {
    await page.goto('/app/job-copy?owner=11');
    await expect(page.getByRole('heading', { name: 'ホーム', exact: true })).toBeVisible();
    await page.locator('.jh-card').nth(index).click();
    await audit(page, `${prefix}-card-${index}`);
  }
  await page.goto('/app/job-copy?view=deals&owner=11');
  await expect(page.locator('.db-table')).toBeVisible();
  await audit(page, `${prefix}-deals`);
  await page.locator('.db-table thead button').first().click();
await audit(page, `${prefix}-sort`);
  await page.getByRole('checkbox', { name: '終わった案件も出す' }).check();
  await page.getByRole('combobox', { name: 'パイプライン', exact: true }).selectOption('743961896');
  await audit(page, `${prefix}-filtered`);
  await page.getByRole('combobox', { name: 'パイプライン', exact: true }).selectOption('all');
  if (rows.length) { await page.locator('.db-table tbody button[aria-expanded]').first().click(); await audit(page, `${prefix}-expanded`); }
  await page.goto('/app/job-copy?deal=100');
  await expect(page.getByRole('heading', { name: 'この取引の求人', exact: true })).toBeVisible();
  await expect(page.getByRole('table', { name: '取引の求人一覧表' })).toBeVisible();
  await expect(page.locator('.dc-fields')).toBeVisible();
  await audit(page, `${prefix}-deal`);
  await page.locator('.dc-root').getByRole('button', { name: '編集', exact: true }).click();
  await audit(page, `${prefix}-contract-edit`);
  await page.locator('.dc-root').getByRole('button', { name: 'キャンセル', exact: true }).click();
  await page.getByRole('button', { name: '次の予定を入れる', exact: true }).click();
  await audit(page, `${prefix}-next-todo`);
  await page.getByRole('dialog', { name: '次の予定を入れる' }).getByRole('button', { name: '閉じる', exact: true }).click();
  const selection = page.locator('.jc-deal-table input[type=checkbox]');
  for (let i = 0; i < Math.min(5, listings.length); i++) await selection.nth(i).check();
  await audit(page, `${prefix}-deal-selection`);
  if (listings.length >= 2) { await page.getByRole('button', { name: '横に並べて比べる' }).click(); await audit(page, `${prefix}-deal-compare`); }
  await page.goto('/app/job-copy?company=900&owner=11');
  await expect(page.locator('.jc-main')).not.toContainText('求人を読み込んでいます…');
  await audit(page, `${prefix}-company`);
  await page.getByRole('button', { name: '求人一覧を開く', exact: true }).click();
  await page.getByRole('button', { name: '固定', exact: true }).click();
  await audit(page, `${prefix}-company-sidebar`);
  await page.getByRole('button', { name: '固定を解除', exact: true }).click();
  await page.goto('/app/job-copy?view=occupations');
  await expect(page.getByText('市場と比べる職種を指定する', { exact: true })).toBeVisible();
  await expect(page.locator('.ob-table tbody tr')).toHaveCount(listings.length || 1);
  await audit(page, `${prefix}-occupations`);
  for (const tab of await page.getByRole('tab').all()) { await tab.click(); await audit(page, `${prefix}-occupation-${await tab.textContent()}`); }
  if (!listings.length) {
    await page.goto('/app/job-copy?owner=11');
    await page.getByRole('button', { name: '求人一覧を開く', exact: true }).click();
    await page.getByRole('button', { name: '固定', exact: true }).click();
    await findListings(page);
    await expect(page.locator('.jc-listings-panel')).toBeVisible();
    await audit(page, `${prefix}-empty-listings`);
    return;
  }
  await page.goto('/app/job-copy?listing=1');
  await expect(page.locator('[data-testid="lifeline"]')).toBeVisible();
  await expect(page.locator('.lf-marker')).toHaveCount(2);
  await audit(page, `${prefix}-listing`);
  await page.locator('.mo-open:visible').click();
  const picker = page.locator('.mo-drawer');
  await expect(picker.locator('.mo-option').last()).toBeVisible();
  await audit(page, `${prefix}-occupation-picker`);
  expect.soft(await picker.locator('.mo-body').evaluate(el => el.scrollWidth - el.clientWidth)).toBeLessThanOrEqual(1);
  await picker.getByLabel('職種を探す', { exact: true }).fill('存在しない架空の職種');
  await expect(picker.getByText('一致する職種はありません。', { exact: true })).toBeVisible();
  await picker.getByLabel('市場と比べない', { exact: true }).check();
  await audit(page, `${prefix}-occupation-picker-empty`);
  await picker.getByRole('button', { name: 'やめる', exact: true }).click();
  await page.getByRole('button', { name: '求人一覧を開く', exact: true }).click();
  await page.getByRole('button', { name: '固定', exact: true }).click();
  await findListings(page);
  await expect(page.locator('.jc-hubspot-job')).toHaveCount(listings.length);
  await audit(page, `${prefix}-pinned`);
  await page.locator('.jc-listings-panel').getByLabel('並び順', { exact: true }).selectOption('applications');
  await audit(page, `${prefix}-listing-sort`);
  await page.getByRole('button', { name: '固定を解除', exact: true }).click();
  if (await page.locator('.lo-event-head').count()) { await page.locator('.lo-event-head').first().click(); await audit(page, `${prefix}-selected-week`); }
  ended = true;
  await page.reload();
  await expect(page.locator('[data-testid="lifeline"]')).toBeVisible();
  await expect(page.locator('.lf-marker-end')).toHaveCount(1);
  await audit(page, `${prefix}-ended`);
  await page.getByRole('button', { name: 'この求人から新しいパターンを作る', exact: true }).click();
  await expect(page.getByRole('region', { name: '求人の手直し', exact: true })).toBeVisible();
  await audit(page, `${prefix}-new-pattern`);
  await page.getByRole('button', { name: '手直しを閉じる', exact: true }).click();
  await page.getByRole('button', { name: '求人の内容を確認する' }).click();
  await audit(page, `${prefix}-popup`);
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'データ取込', exact: true }).click();
  await audit(page, `${prefix}-import`);
  await page.getByRole('button', { name: '外部文面を照合する', exact: true }).click();
  await expect(page.locator('dialog[open]')).toBeVisible();
  await audit(page, `${prefix}-receive`);
  await page.goto('/app/job-copy?listing=1&dialog=band');
  await expect(page.locator('dialog[open]')).toBeVisible();
  await audit(page, `${prefix}-full-text`);
  await page.getByRole('button', { name: '全文', exact: true }).click();
  await audit(page, `${prefix}-full-text-all`);
  for (const drawer of ['who', 'billing', 'market']) {
    await page.goto(`/app/job-copy?listing=1&drawer=${drawer}`);
    await expect(page.locator('dialog[open]')).toBeVisible();
    await audit(page, `${prefix}-drawer-${drawer}`);
  }
  await page.goto('/app/job-copy?listing=1&analysis=1&feature=timeline');
  await expect(page.locator('dialog[open]')).toBeVisible();
  for (const group of ['タイムライン', '求人内容', '応募分析', '市場分析', '比較・報告']) {
    await page.getByRole('tab', { name: group, exact: true }).click();
    if (group === 'タイムライン' && scenario.weeks > 1) {
      for (const id of ['jt-applications', 'jt-market']) expect((await getChartSeriesLengths(page, id)).some(n => n > 0)).toBe(true);
    }
    await audit(page, `${prefix}-${group}`);
    for (const tab of await page.locator('.jc-feature-secondary:visible [role="tab"]').all()) { await tab.click(); await audit(page, `${prefix}-${await tab.textContent()}`); }
  }
});

test('文字が重なると検査結果が空にならず失敗する', async ({ page }) => {
  await page.setContent('<main><span style="position:absolute;left:10px;top:10px">架空の文字一</span><span style="position:absolute;left:10px;top:10px">架空の文字二</span></main>');
  const report = await findLayoutProblems(page, 'main');
  expect(report.overlaps).toHaveLength(1);
  expect(() => expect(report).toEqual(emptyReport)).toThrow();
});

test('閉じた説明と固定帯の背後を除き、はみ出しと図の切れは検知する', async ({ page }) => {
  await page.setContent(`<main>
    <details><summary>説明</summary><span style="position:absolute;left:0;top:0">隠れた文字</span></details>
    <div style="position:absolute;left:0;top:80px">帯の背後の文字</div>
    <div style="position:fixed;left:0;top:80px;background:white;z-index:1">固定帯の文字</div>
    <div style="position:absolute;left:0;top:140px;width:20px;white-space:nowrap">枠から出る長い文字</div>
    <svg style="position:absolute;left:0;top:200px" width="30" height="30"><text x="20" y="20">図から出る文字</text></svg>
  </main>`);
  const report = await findLayoutProblems(page, 'main');
  expect(report.overlaps).toEqual([]);
  expect(report.spills.length).toBeGreaterThan(0);
  expect(report.svgClipped).toHaveLength(1);
  await page.setContent('<main><span style="position:fixed;top:-100px">画面の外の文字一</span><span style="position:fixed;top:-100px">画面の外の文字二</span></main>');
  expect((await findLayoutProblems(page, 'main')).overlaps).toEqual([]);
});

for (const width of [1280, 1440]) test(`固定一覧・横断比較・応募者の条件・課金CSV ${width}`, async ({ page }) => {
  test.setTimeout(120_000);
  await page.setViewportSize({ width, height: 1000 });
  await page.addInitScript(() => localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'));
  await login(page);
  await page.goto('/app/job-copy?demo=1');
  await expect(page.locator('.jc-job').first()).toBeVisible();
  await audit(page, `${width}-demo-list`);
  await page.locator('.jc-job', { hasText: '倉庫内ピッキングスタッフ' }).click();
  await audit(page, `${width}-demo-airwork`);
  await page.getByRole('button', { name: '横断比較', exact: true }).click();
  await expect(page.getByRole('region', { name: '求人の横断比較の表' })).toBeVisible();
  await audit(page, `${width}-demo-overview`);
  await page.getByRole('button', { name: '一覧', exact: true }).click();
  await page.getByRole('button', { name: '応募者の条件で探す', exact: true }).click();
  await audit(page, `${width}-demo-reverse`);
  await page.getByRole('button', { name: 'データ取込', exact: true }).click();
  const csv = '媒体,店舗ID,媒体求人ID,期間開始,期間終了,金額（円・税込）\n' + Array.from({ length: 80 }, (_, i) => `Airワーク,SYNTHETIC,${i},2026-10-01,2026-10-18,987654321`).join('\n');
  await page.getByLabel('課金CSVファイル', { exact: true }).setInputFiles({ name: 'synthetic-billing.csv', mimeType: 'text/csv', buffer: Buffer.from(csv) });
  await page.getByRole('button', { name: '求人と照合する', exact: true }).click();
  await audit(page, `${width}-demo-billing-csv`);
});

for (const width of [1280, 1440]) test(`数値のある1週の求人で印と応募数を重ねない ${width}`, async ({ page }) => {
  await page.setViewportSize({ width, height: 1000 });
  await page.clock.setFixedTime(new Date('2026-10-18T03:00:00Z'));
  const listing = { ...fixtureJobs[0], media_job_id: '12345678', title: '架空の求人', listing_started_on: '2026-10-12', publication_status: '掲載中', application_count: 0 };
  await page.route('**/api/job-copy/**', route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/versions')) return route.fulfill({ json: { ...fixtureHistory('1'), listing, versions: [{ written_at: '2026-10-12T00:00:00Z', body: '仕事内容：架空の作業です。', image_urls: [] }], indeed_weekly_perf: { schema_version: 1, source: 'hrhacker', job_id: '12345678', captured_at: '2026-10-18T00:00:00Z', rows: [{ period_start: '2026-10-12', period_end: '2026-10-18', impressions: 987654321, clicks: 987654321, cost_yen: 987654321, applications: 987654321 }] } } });
    if (path.endsWith('/applicants')) return route.fulfill({ json: { listing_id: '1', total: 0, missing_date: 0, by_date: {}, fetched_at: '2026-10-18T00:00:00Z' } });
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  await login(page);
  await page.goto('/app/job-copy?listing=1');
  await expect(page.locator('.lf-marker')).toHaveCount(2);
  await expect(page.locator('.lf-row[aria-label="応募数（Indeedの集計）"]')).toHaveText('987,654,321');
  await page.locator('.lo-event-head').first().click();
  await expect(page.locator('[data-testid="lifeline-selected"]')).toHaveCount(1);
  await audit(page, `${width}-one-week-numeric-markers`);
});
