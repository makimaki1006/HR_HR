import path from 'node:path';
import { defineConfig } from '@playwright/test';
import consultant from './job-copy-consultant-risk.config';

// 3画面の単独実行も、PRと同じ認証付きRust / fixture DBで検証する。
export default defineConfig({
  ...consultant,
  testMatch: ['job_copy_consultant_risk.spec.ts', 'job_copy_consultant_good.spec.ts', 'job_copy_consultant_houjin.spec.ts'],
  outputDir: path.resolve(__dirname, '../../test-results/consultant-houjin'),
});
