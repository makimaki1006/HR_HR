import { APIRequestContext, BrowserContext, Page, expect, request } from '@playwright/test';
import { LIVE } from './live';

/** 偽 Google で本物のサーバにログインし、Cookie をブラウザへ渡す (scripts/loadtest/run.mjs の loginVu と同じ手順) */
export async function loginAs(context: BrowserContext, email: string = LIVE.userEmail): Promise<void> {
  const api: APIRequestContext = await request.newContext({ baseURL: LIVE.app });
  try {
    let r = await api.get('/auth/google/login', { maxRedirects: 0 });
    expect([302, 303]).toContain(r.status());
    const authz = r.headers()['location'];
    r = await api.get(`${authz}&login_hint=${encodeURIComponent(email)}`, { maxRedirects: 0 });
    const cb = r.headers()['location'];
    r = await api.get(cb, { maxRedirects: 0 });
    expect(r.status()).toBe(200);
    const state = await api.storageState();
    await context.addCookies(state.cookies);
  } finally {
    await api.dispose();
  }
}

const j = async (path: string, init?: RequestInit) => {
  const r = await fetch(`${LIVE.hubspot}${path}`, init);
  return r.json() as Promise<any>; // eslint-disable-line @typescript-eslint/no-explicit-any
};

/** 偽 HubSpot の今のレコード (HubSpot の API を通さず、偽サーバの内部状態を直接見る) */
export async function hsRecord(object: 'deals' | 'contacts' | 'companies', id: string): Promise<Record<string, string | null>> {
  return (await j(`/_record?object=${object}&id=${id}`)).properties;
}
/** 偽 HubSpot が受けた PATCH の記録 */
export async function hsPatches(): Promise<{ object: string; id: string; properties: Record<string, string>; status: number }[]> {
  return (await j('/_patches')).patches;
}
/** 次の PATCH を n 回 status で失敗させる */
export async function hsFail(count: number, status: number): Promise<void> {
  await j('/_fail', { method: 'POST', body: JSON.stringify({ count, status }) });
}
/** HubSpot 側で別の人が値を変えた状態にする */
export async function hsMutate(object: 'deals' | 'contacts' | 'companies', id: string, properties: Record<string, string>): Promise<void> {
  await j('/_mutate', { method: 'POST', body: JSON.stringify({ object, id, properties }) });
}
/** 偽 Turso (監査 DB) を落とす / 戻す */
export async function tursoDown(down: boolean): Promise<void> {
  await fetch(`${LIVE.turso}/_down`, { method: 'POST', body: JSON.stringify({ down }) });
}
export async function tursoQuery(sql: string): Promise<Record<string, unknown>[]> {
  const r = await fetch(`${LIVE.turso}/_query`, { method: 'POST', body: JSON.stringify({ sql }) });
  return ((await r.json()) as { rows: Record<string, unknown>[] }).rows;
}

/** 架電キューから、会社名が一致する行を選んで詳細を開く */
export async function openDeal(page: Page, companyName: string): Promise<void> {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto('/app/crm');
  const list = page.getByRole('list', { name: '架電キュー' });
  const row = list.locator('li.cq-row button.cq-row-button', { hasText: companyName });
  await expect(row.first()).toBeVisible();
  await row.first().click();
  await expect(page.getByRole('article', { name: '架電先の詳細' })).toBeVisible();
}

export const COMPANY_A = 'E2E書込テスト会社A';
export const COMPANY_B = 'E2E書込テスト会社B';

/** 偽 HubSpot の案件・担当者を、各テストの初期値に戻す (テスト同士が影響し合わないように) */
export async function resetState(): Promise<void> {
  await hsMutate('deals', LIVE.allowedDealId, { bpo_29: '', bpo_24: '', bpo_10: '', bpo_31: '', dealstage: '1095387442', pipeline: '753186575' });
  await hsMutate('contacts', '7000000000', { jobtitle: '総務部長' });
}

/** プロパティのタブを開き、BPOアポ情報の折りたたみを開く */
export async function openPropertyPanel(page: Page) {
  await page.getByRole('tab', { name: 'プロパティ' }).click();
  const panel = page.getByTestId('property-panel');
  const header = panel.getByText('BPOアポ情報');
  await expect(header).toBeVisible();
  if (!(await panel.getByText('架電先電話番号').isVisible())) await header.click();
  await expect(panel.getByText('架電先電話番号')).toBeVisible();
  return panel;
}

/** 項目の「編集」を押し、値を入れて「保存」まで押す (結果の確認は呼び出し側) */
export async function editText(panel: ReturnType<Page['getByTestId']>, label: string, value: string, save = true): Promise<void> {
  await panel.getByRole('button', { name: `${label}を編集` }).click();
  await panel.getByRole('textbox', { name: label }).fill(value);
  if (save) await panel.getByRole('button', { name: '保存', exact: true }).click();
}
