import { expect, Page, Route, test } from '@playwright/test';
import { login } from './helpers/login';
import { MOC_DEAL_PROPERTIES } from '../../../frontend/src/screens/crm/mocProperties';
import { QUEUE_STAGE_IDS } from '../../../frontend/src/screens/crm/queueModel';

/**
 * 架電画面 (/app/crm、実データの表示) の守り:
 * - 一覧を読み直している間・読めなかったときは「記録して次へ」で記録しない (一覧で見えない案件を記録しない)
 * - 選択肢 (HubSpot の項目定義) を読めなかったとき、原因に合った案内を出し、「HubSpot で直接入力」は案内しない
 *
 * password ログインでは /api/crm/* は 403 になるため、/api/crm/* はすべて page.route で架空の応答を返す
 * (HubSpot へは接続しない)。Zoom の枠 (applications.zoom.us) も読み込まない。
 * この画面にグラフ (ECharts) は無い。値は画面の文字と sessionStorage の中身で確かめる。
 */

const STORAGE_KEY = 'hrhr.crm.callResult.v1';

function item(id: string) {
  return {
    deal_id: id, deal_name: `架空案件${id}`, stage_id: '1095387442', stage_label: '未済', owner_id: null,
    next_call_date: null, next_call_time: null, last_call_date: null,
    stop: { prohibited_reason: null, block_reason: null, unreachable_check: null },
    contact: { id: `c${id}`, name: `架空 太郎${id}`, phone: '+81300000001', mobile: null, job_title: '採用担当', extra_count: 0 },
    company: { id: `co${id}`, name: `架空会社${id}`, phone: null },
    phone: '+81300000001', phone_source: 'contact',
    deep_links: { deal: null, contact: null, company: null },
  };
}

/** 要求の条件 (URL) に合わせた scope の応答 (画面は scope が条件と一致しないと表示しない) */
function queueBody(url: URL, ids: string[]) {
  const p = url.searchParams;
  return {
    items: ids.map(item), next_cursor: null, total: ids.length, truncated: false,
    scope: {
      owner: 'all', role: 'admin', teams: [], stages: [...QUEUE_STAGE_IDS].sort(),
      due: p.get('due') ?? 'all', sort: p.get('sort') ?? 'default', q: null, limit: 25,
      next_from: null, next_to: null, last_from: null, last_to: null,
    },
    partial: { missing_contacts: 0, missing_companies: 0, failed: [], excluded: { no_phone: 0, stop_reason: 0, out_of_scope: 0 } },
    generated_at: '2026-10-08T03:00:00Z',
  };
}

const BPO_57 = { name: 'bpo_57', label: 'その他理由', type: 'string', fieldType: 'text', options: [] };
function metadataBody() {
  return {
    properties: [...Object.values(MOC_DEAL_PROPERTIES), BPO_57].map((p) => ({
      object_type: 'deals', name: p.name, label: p.label, property_type: p.type, field_type: p.fieldType, options: p.options,
    })),
    pipelines: [], fetched_at: '2026-10-08T00:00:00Z', hubspot_ms: 1, total_ms: 1, cache_hit: false,
  };
}

interface Mock { held: Route[]; metadataCalls: number; writes: string[] }

/** /api/crm/* を架空の応答にする。due=today の一覧は held に溜めて、テストが返すまで止める */
async function mockCrm(page: Page, metadataFirst: { status: number; kind: string }): Promise<Mock> {
  const m: Mock = { held: [], metadataCalls: 0, writes: [] };
  await page.route(/applications\.zoom\.us/, (r) => r.abort());
  await page.route('**/api/crm/**', async (route) => {
    const req = route.request();
    const url = new URL(req.url());
    if (req.method() !== 'GET') m.writes.push(`${req.method()} ${url.pathname}`);
    if (url.pathname === '/api/crm/call-queue') {
      if (url.searchParams.get('due') === 'today') { m.held.push(route); return; }
      await route.fulfill({ json: queueBody(url, ['1', '2']) });
      return;
    }
    if (url.pathname === '/api/crm/metadata') {
      m.metadataCalls += 1;
      if (m.metadataCalls === 1) { await route.fulfill({ status: metadataFirst.status, json: { error: 'x', error_kind: metadataFirst.kind } }); return; }
      await route.fulfill({ json: metadataBody() });
      return;
    }
    // 所有者・詳細は今回の確認に使わない (失敗のまま)
    await route.fulfill({ status: 502, json: { error: 'x', error_kind: 'hubspot_upstream' } });
  });
  return m;
}

const list = (page: Page) => page.getByRole('list', { name: '架電キュー' });
const form = (page: Page) => page.getByRole('form', { name: '架電結果の入力' });
const recordBtn = (page: Page) => form(page).getByRole('button', { name: /記録して次へ/ });
const recorded = (page: Page) => page.evaluate((k) => (JSON.parse(window.sessionStorage.getItem(k) ?? '{"recorded":{}}') as { recorded: Record<string, boolean> }).recorded, STORAGE_KEY);

test.describe('CRM 架電画面 (実データの表示、API は架空の応答): 記録の守りと案内', () => {
  test.beforeEach(async ({ page }) => { await login(page); });

  test('選択肢の読み込みが上限で失敗 → 待って再試行の案内 (直接入力は案内しない)。一覧の読み直し中・失敗時は記録しない', async ({ page }) => {
    const m = await mockCrm(page, { status: 503, kind: 'hubspot_rate_limited' });
    await page.goto('/app/crm');
    await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('実データ(HubSpot)');
    await expect(list(page).locator('.cq-row-company')).toHaveText(['架空会社1', '架空会社2']);
    await list(page).getByText('架空会社1').click();

    // 選択肢の取得が上限で失敗: 原因と、利用者ができること
    const alert = form(page).getByRole('alert');
    await expect(alert).toContainText('HubSpot から選択肢を読み込めませんでした。HubSpot の呼び出し回数の上限に達しています。少し待ってから再試行してください。');
    await expect(alert).toContainText('再試行しても表示されないときは管理者に連絡してください。');
    await expect(alert).not.toContainText('直接入力');
    await alert.getByRole('button', { name: '再試行' }).click();
    const outcomes = form(page).getByRole('group', { name: '今回の結果' });
    await expect(outcomes.getByRole('button')).toHaveText(['担当者と会話', '不在・応答なし', '再架電の約束', 'アポイント獲得', '番号違い', '架電停止の希望']);
    expect(m.metadataCalls).toBe(2);

    await outcomes.getByRole('button', { name: '担当者と会話', exact: true }).click();
    await expect(recordBtn(page)).toHaveAttribute('aria-disabled', 'false');

    // 条件を変えて一覧を読み直している間は記録できない
    await page.getByRole('checkbox', { name: '次回日が来たものだけ' }).check();
    await expect(page.getByText('読み込み中…', { exact: true })).toBeVisible();
    await expect.poll(() => m.held.length).toBe(1);
    await expect(recordBtn(page)).toHaveAttribute('aria-disabled', 'true');
    await expect(form(page).locator('.rf-notice')).toHaveText('一覧を読み込み中です。一覧が表示されてから記録してください。');
    await recordBtn(page).click({ force: true });
    await form(page).getByLabel(/^タスクメモ/).press('Control+Enter');
    await expect(page.getByTestId('result-slot')).toHaveAttribute('data-deal-id', '1');
    await expect(form(page).locator('.rf-notice')).not.toHaveText('表示中の一覧に未記録の架電先はありません。');
    expect(await recorded(page)).toEqual({});

    // 読み直しが失敗しても記録できない
    await m.held[0]?.fulfill({ status: 502, json: { error: 'x', error_kind: 'hubspot_upstream' } });
    const listCol = page.getByRole('region', { name: '架電先の一覧' });
    await expect(listCol.getByRole('alert')).toContainText('取得できませんでしたHubSpot との通信に失敗しました。再試行してください。');
    await expect(form(page).locator('.rf-notice')).toHaveText('一覧を表示できていないため記録できません。一覧を表示してから記録してください。');
    await recordBtn(page).click({ force: true });
    expect(await recorded(page)).toEqual({});

    // 一覧が表示されれば (案件 1 が載っている) 記録でき、次の案件へ進む
    await listCol.getByRole('button', { name: '再試行' }).click();
    await expect.poll(() => m.held.length).toBe(2);
    const second = m.held[1];
    if (!second) throw new Error('no second request');
    await second.fulfill({ json: queueBody(new URL(second.request().url()), ['1', '2']) });
    await expect(list(page).locator('.cq-row-company')).toHaveText(['架空会社1', '架空会社2']);
    await expect(recordBtn(page)).toHaveAttribute('aria-disabled', 'false');
    await recordBtn(page).click();
    await expect(list(page).locator('li.cq-row').nth(0).locator('.cq-recorded')).toHaveText('記録済み(未送信)');
    await expect(page.getByTestId('result-slot')).toHaveAttribute('data-deal-id', '2');
    expect(await recorded(page)).toEqual({ 'live:1': true });

    // HubSpot への書き込み (GET 以外) は 0 件
    expect(m.writes).toEqual([]);
  });
});
