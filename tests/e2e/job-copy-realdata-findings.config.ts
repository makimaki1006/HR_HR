import { defineConfig } from '@playwright/test';
import base from './job-copy-deal-compare.config';
export default defineConfig({ ...base, testMatch: [...(base.testMatch as string[]), 'job_copy_realdata_findings.spec.ts', 'rikurozi_draft.spec.ts'] });
