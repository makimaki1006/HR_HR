import { test } from '@playwright/test';
import { login } from './helpers/login';
import { registerDealCompareTests } from '../job-copy-deal-compare-scenarios';
registerDealCompareTests('/app/job-copy');

test.beforeEach(async ({ page }) => { await login(page); });
