import { defineConfig } from '@playwright/test';
import base from './job-copy-manage-edit-flow.config';
export default defineConfig({ ...base, testMatch: 'job_copy_contract.spec.ts', outputDir: '../../data/job-copy-local/contract/test-results' });
