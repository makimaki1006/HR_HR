import { defineConfig } from '@playwright/test';
const baseURL = `http://localhost:${process.env.E2E_PR_PORT ?? '9217'}`;

export default defineConfig({
  testDir: '.', testMatch: 'crm-stage-contact.spec.ts',
  globalSetup: '../../tests/e2e/pr/global-setup.ts',
  workers: 1, timeout: 60_000,
  outputDir: process.env.CRM_TEST_ARTIFACTS ?? '../../../crm-stage-contact-artifacts/e2e',
  reporter: 'list',
  use: { baseURL, viewport: { width: 1280, height: 800 }, screenshot: 'only-on-failure' },
});
