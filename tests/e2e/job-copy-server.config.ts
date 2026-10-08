import { defineConfig, devices } from '@playwright/test';
import { JOB_COPY_BASE_URL, JOB_COPY_STATE } from './job-copy-server-setup';

/**
 * The job-copy specs that run on synthetic data only, against the Rust server with a login.
 * Run (after `cd frontend && npm run build` and `cargo build --bin rust_dashboard`):
 *   E2E_BIN=<target>/debug/rust_dashboard E2E_FIXTURE_DB=<path>/hellowork.db \
 *     npx playwright test -c tests/e2e/job-copy-server.config.ts
 * E2E_PR_PORT changes the port (default 9217) when another work tree runs E2E at the same time.
 * job_copy_candidate.spec.ts and job_copy_actual_reactions.spec.ts are not here: they need
 * private local data (job-copy-candidate-preview / job-copy-reaction-preview configs).
 */
export default defineConfig({
  testDir: '.',
  testMatch: [
    'job_copy_load_recovery.spec.ts', 'job_copy_progressive_loading.spec.ts', 'job_copy_candidate_errors.spec.ts',
    'job_copy_candidate_reasons.spec.ts', 'job_copy_navigation.spec.ts', 'job_copy_analysis.spec.ts', 'job_copy_ab_comparison.spec.ts',
  ],
  globalSetup: './job-copy-server-setup.ts',
  workers: 1,
  retries: 0,
  timeout: 120_000,
  reporter: 'list',
  outputDir: '../../test-results-job-copy',
  use: {
    ...devices['Desktop Chrome'],
    baseURL: JOB_COPY_BASE_URL,
    storageState: JOB_COPY_STATE,
    trace: 'off', screenshot: 'off', video: 'off',
    launchOptions: process.env.E2E_CHROME_PATH ? { executablePath: process.env.E2E_CHROME_PATH } : {},
  },
});
