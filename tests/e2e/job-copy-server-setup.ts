import * as os from 'node:os';
import * as path from 'node:path';
import { chromium } from '@playwright/test';
import prGlobalSetup from './pr/global-setup';
import { E2E_EMAIL, E2E_PASSWORD, PR_BASE_URL, PR_PORT } from './pr/helpers/fixture_values';

/**
 * Start the Rust server and log in once, for the job-copy specs that need only synthetic data
 * (they replace /api/job-copy/* with page.route). Used by job-copy-server.config.ts.
 *
 * - With JOB_COPY_BASE_URL unset, the server is started like the PR E2E (tests/e2e/pr/global-setup.ts:
 *   E2E_BIN, E2E_FIXTURE_DB, E2E_PR_PORT) and stopped afterwards.
 * - With JOB_COPY_BASE_URL set, that running server is used (it must accept AUTH_PASSWORD=testpass
 *   for e2e@f-a-c.co.jp, as the PR E2E server does).
 * The login cookie is saved to JOB_COPY_STATE and given to every test.
 */
export const JOB_COPY_BASE_URL = process.env.JOB_COPY_BASE_URL ?? PR_BASE_URL;
export const JOB_COPY_STATE = path.join(os.tmpdir(), `job-copy-e2e-login-${String(PR_PORT)}.json`);

export default async function setup(): Promise<(() => Promise<void>) | undefined> {
  const teardown = process.env.JOB_COPY_BASE_URL ? undefined : await prGlobalSetup();
  const browser = await chromium.launch(process.env.E2E_CHROME_PATH ? { executablePath: process.env.E2E_CHROME_PATH } : {});
  try {
    const page = await browser.newPage({ baseURL: JOB_COPY_BASE_URL });
    await page.goto('/login');
    await page.fill('#email', E2E_EMAIL);
    await page.fill('#password', E2E_PASSWORD);
    await Promise.all([
      page.waitForURL(url => url.pathname === '/'),
      page.click("form[action='/login'] button[type=submit]"),
    ]);
    await page.context().storageState({ path: JOB_COPY_STATE });
  } catch (error) {
    await teardown?.();
    throw error;
  } finally {
    await browser.close();
  }
  return teardown;
}
