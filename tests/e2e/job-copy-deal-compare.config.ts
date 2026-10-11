import { defineConfig } from '@playwright/test';
import base from './job-copy-listings.config';
export default defineConfig({ ...base, testMatch: ['job_copy_deal_compare.spec.ts', 'job_copy_listings.spec.ts', 'job_copy_listings_workspace.spec.ts', 'job_copy_sidebar.spec.ts', 'job_copy_ui_polish.spec.ts'] });
