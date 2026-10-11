import { test } from '@playwright/test';
import { login } from './helpers/login';
import { fixtureJobs, fixtureHistory } from '../job-copy-listings-fixture.mjs';
import { registerRealdataFindingsTests } from '../job-copy-realdata-findings-scenarios';
test.beforeEach(async ({ page }) => {
  await login(page);
  await page.route('**/api/job-copy/listings?*', route => route.fulfill({ json: { status: 'ready', total: fixtureJobs.length, index_built_at: '2026-10-10T00:00:00Z', listings: fixtureJobs, titles: [], offset: 0, next_offset: null, refreshing: false, refresh_failed: false } }));
  await page.route('**/api/job-copy/listings/*/versions', route => route.fulfill({ json: fixtureHistory(new URL(route.request().url()).pathname.split('/').at(-2)) }));
});
registerRealdataFindingsTests();
