import { expect, Page } from '@playwright/test';
import { E2E_EMAIL, E2E_PASSWORD } from './fixture_values';

/** ログインして / のダッシュボード (ヘッダーの「ログイン: <email>」) が出るまで待つ。 */
export async function login(page: Page): Promise<void> {
  await page.goto('/login');
  await page.fill('#email', E2E_EMAIL);
  await page.fill('#password', E2E_PASSWORD);
  await Promise.all([
    page.waitForURL((u) => u.pathname === '/'),
    page.click("form[action='/login'] button[type=submit]"),
  ]);
  await expect(page.getByText(`ログイン: ${E2E_EMAIL}`)).toBeVisible();
}

/**
 * 認証後の POST は Origin が許可リスト (localhost:3000/8080 等、src/lib.rs ALLOWED_ORIGINS) に無いと 403。
 * ブラウザの fetch は Origin を上書きできないため、POST は page.request (ログイン済み cookie を共有) に
 * Origin: http://localhost:8080 を付けて送る。React の POST には X-Requested-With: fetch も付ける。
 */
export async function postJson(page: Page, url: string, body: unknown) {
  return page.request.post(url, {
    data: body,
    headers: { Origin: 'http://localhost:8080', 'X-Requested-With': 'fetch' },
  });
}
