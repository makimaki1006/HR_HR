import { defineConfig } from '@playwright/test';
import base from './job-copy-manage-edit-flow.config';
/** 顧客報告のスライド（架空のデータを page.route で返す。Vite の確認用サーバーで動く）。 */
export default defineConfig({
  ...base,
  testMatch: 'job_copy_report_slides.spec.ts',
  use: { ...base.use, locale: 'ja-JP', timezoneId: 'Asia/Tokyo' },
  outputDir: '../../data/job-copy-local/report-slides/test-results',
});
