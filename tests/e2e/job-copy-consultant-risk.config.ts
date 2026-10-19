import path from 'node:path';
import { defineConfig } from '@playwright/test';
import pr from './pr/playwright.pr.config';

// 単独実行も PR と同じ認証済み Rust / fixture DB を使う。E2E_BIN 等は PR 設定と共通。
export default defineConfig({
  ...pr,
  testDir: path.resolve(__dirname, 'pr'),
  testMatch: 'job_copy_consultant_risk.spec.ts',
  globalSetup: path.resolve(__dirname, 'pr/global-setup.ts'),
  outputDir: path.resolve(__dirname, '../../test-results/consultant-risk'),
  retries: 0,
  reporter: 'list',
  use: { ...pr.use, trace: 'off' },
  projects: [{ name: 'chromium', use: { viewport: { width: 1440, height: 900 } } }],
});
