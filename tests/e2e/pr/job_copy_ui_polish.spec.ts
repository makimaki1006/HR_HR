import { test } from '@playwright/test';
import { login } from './helpers/login';
import { fixtureJobs, fixtureHistory } from '../job-copy-listings-fixture.mjs';
import '../job_copy_ui_polish.spec';
// Run the same concrete assertions in CI against the authenticated Rust shell.
// All job-copy reads are synthetic; no live HubSpot or LLM request is allowed.
test.beforeEach(async ({ page }) => {
  await login(page);
  await page.route('**/api/job-copy/**', route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/listings')) {
      const rows = fixtureJobs.filter(row => !url.searchParams.get('prefecture') || row.prefecture === url.searchParams.get('prefecture'));
      return route.fulfill({ json: { status: 'ready', total: rows.length, index_built_at: '2026-10-10T00:00:00Z', listings: rows, titles: ['ドライバー', '看護師'], offset: 0, next_offset: null, refreshing: false, refresh_failed: false } });
    }
    if (url.pathname.endsWith('/versions')) return route.fulfill({ json: fixtureHistory('1') });
    if (url.pathname.endsWith('/market')) return route.fulfill({ json: { titles: [], prefectures: [], series: null } });
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
});
