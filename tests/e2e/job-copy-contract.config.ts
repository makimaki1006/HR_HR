import { defineConfig } from '@playwright/test';
import base from './job-copy-manage-edit-flow.config';
export default defineConfig({
  ...base,
  testMatch: 'job_copy_contract.spec.ts',
  use: {
    ...base.use,
    locale: 'ja-JP',
    timezoneId: 'Asia/Tokyo',
    // ネイティブの日付入力も日本式にするため、Chromium 本体の言語もそろえる。
    launchOptions: { args: ['--lang=ja-JP'], env: { ...process.env, LANG: 'ja_JP.UTF-8', LC_ALL: 'ja_JP.UTF-8' } },
  },
  outputDir: '../../data/job-copy-local/contract/test-results',
});
