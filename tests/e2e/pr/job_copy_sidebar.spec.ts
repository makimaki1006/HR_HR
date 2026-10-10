import { test } from '@playwright/test';
import { login } from './helpers/login';
import { sidebarTests } from '../job-copy-sidebar-tests';
test.beforeEach(async ({ page }) => { await login(page); });
sidebarTests();
