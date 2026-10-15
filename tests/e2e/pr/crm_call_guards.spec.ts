import { openQueueToolbar } from './helpers/crm_toolbar';
import { expect, Page, Route, test } from '@playwright/test';
import { login } from './helpers/login';
import { startWithResultPanelInFront } from './helpers/crm_layout';
import { MOC_DEAL_PROPERTIES } from '../../../frontend/src/screens/crm/mocProperties';
import { QUEUE_STAGE_IDS } from '../../../frontend/src/screens/crm/queueModel';

/**
 * 架電画面 (/app/crm、実データの表示) の守り:
 * - 一覧を読み直している間・読めなかったときは「記録して次へ」で保存しない (一覧で見えない案件を保存して次へ進まない)
 * - 一覧が表示されれば、架電結果の入力欄の項目を 1 回の PATCH で保存し、次の案件へ進む
 *
 * password ログインでは /api/crm/* は 403 になるため、/api/crm/* はすべて page.route で架空の応答を返す
 * (HubSpot へは接続しない。PATCH も page.route が受けるだけ)。Zoom の枠 (applications.zoom.us) も読み込まない。
 * この画面にグラフ (ECharts) は無い。値は画面の文字・送った本文・sessionStorage の中身で確かめる。
 */

const STORAGE_KEY = 'hrhr.crm.callResult.v2';
const BPO_PIPELINE = '753186575';

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
      pipeline: BPO_PIPELINE, owner: 'all', role: 'admin', teams: [], stages: [...QUEUE_STAGE_IDS].sort(),
      due: p.get('due') ?? 'all', sort: p.get('sort') ?? 'default', q: null, limit: Number(p.get('limit') ?? 25),
      next_from: null, next_to: null, last_from: null, last_to: null,
    },
    partial: { missing_contacts: 0, missing_companies: 0, failed: [], excluded: { no_phone: 0, stop_reason: 0, out_of_scope: 0 }, unknown_stages: 0 },
    generated_at: '2026-10-08T03:00:00Z',
  };
}

/** 架空の案件の詳細 (架電結果の入力欄の項目は空 = まだ何も入っていない案件) */
function detailBody(id: string, url: URL) {
  const names = (url.searchParams.get('deal_props') ?? '').split(',').filter(Boolean);
  return {
    deal: {
      id, name: `架空案件${id}`, stage_id: '1095387442', stage_label: '未済', pipeline_id: BPO_PIPELINE, owner_id: null,
      amount: null, close_date: null, next_call_date: null, next_call_time: null, last_call_date: null,
      stop: { prohibited_reason: null, block_reason: null, unreachable_check: null }, bpo_phone: null,
      job_search_url: null, homepage_url: null, media_job_urls: null, job_posting_url: null, deep_link: null,
    },
    dial: { number: '+81300000001', source: 'contact' },
    contacts: [], contacts_total: 0, companies: [], companies_total: 0, activities: [], activities_truncated: false,
    activity_scope: '架空', partial: [],
    selected: { deal: Object.fromEntries(names.map(n => [n, null])), contact: {}, company: {} },
    hubspot_portal_id: '0', data_scope: '架空', generated_at: '2026-10-08T03:00:00Z', fetched_at: '2026-10-08T03:00:00Z', cached: false,
  };
}

const prop = (name: string, type: string, field_type: string, options: { value: string; label: string; hidden: boolean }[] = []) =>
  ({ object: 'deal', name, label: MOC_DEAL_PROPERTIES[name]?.label ?? name, type, field_type, options, owner_ref: false, max_length: null });

/** 書ける項目の一覧 (BPO の組の項目。表示名は HubSpot と同じ) */
function schemaBody(id: string) {
  return {
    deal_id: id, pipeline_id: BPO_PIPELINE, stage_id: '1095387442', writes_enabled: true,
    editable: [
      { ...prop('risuto_kadennbi', 'date', 'date'), label: '架電日' }, prop('bpo_13', 'date', 'date'),
      prop('bpo_14', 'enumeration', 'radio', (MOC_DEAL_PROPERTIES.bpo_14?.options ?? []).map(o => ({ value: o.value, label: o.label, hidden: o.hidden === true }))),
      prop('bpo_16', 'string', 'textarea'), { ...prop('bpo_51', 'enumeration', 'select', [{ value: 'true', label: 'はい', hidden: false }]), label: '接触' },
      { ...prop('nitteigiri', 'number', 'number'), label: '日程切り' },
    ],
    stages: [],
    pipelines: [{ id: BPO_PIPELINE, label: 'bpo_リクロジ', stages: [{ id: '1095387442', label: '未済' }, { id: '1095387445', label: '不在' }] }],
  };
}

interface Mock { held: Route[]; patches: { path: string; body: { set: Record<string, string | null>; stage?: unknown } }[] }

/** /api/crm/* を架空の応答にする。due=today の一覧は held に溜めて、テストが返すまで止める */
async function mockCrm(page: Page): Promise<Mock> {
  const m: Mock = { held: [], patches: [] };
  await page.route(/applications\.zoom\.us/, (r) => r.abort());
  await page.route('**/api/crm/**', async (route) => {
    const req = route.request();
    const url = new URL(req.url());
    if (url.pathname === '/api/crm/call-queue') {
      if (url.searchParams.get('due') === 'today') { m.held.push(route); return; }
      await route.fulfill({ json: queueBody(url, ['1', '2']) });
      return;
    }
    const detail = /^\/api\/crm\/workspace\/deals\/([^/]+)$/.exec(url.pathname);
    if (detail?.[1]) { await route.fulfill({ json: detailBody(detail[1], url) }); return; }
    if (url.pathname === '/api/crm/edit-schema') { await route.fulfill({ json: schemaBody(url.searchParams.get('deal_id') ?? '') }); return; }
    if (req.method() === 'PATCH' && url.pathname.startsWith('/api/crm/deals/')) {
      const body = req.postDataJSON() as Mock['patches'][number]['body'];
      m.patches.push({ path: url.pathname, body });
      await route.fulfill({ json: { status: 'saved', values: body.set, objects_values: {}, fetched_at: '2026-10-08T03:00:01Z' } });
      return;
    }
    // 所有者・選択肢・項目の一覧などは今回の確認に使わない (失敗のまま)
    await route.fulfill({ status: 502, json: { error: 'x', error_kind: 'hubspot_upstream' } });
  });
  return m;
}

const list = (page: Page) => page.getByRole('list', { name: '架電キュー' });
const form = (page: Page) => page.getByRole('form', { name: '架電結果の入力' });
const recordBtn = (page: Page) => form(page).getByRole('button', { name: /記録して次へ/ });
const memo = (page: Page) => form(page).getByRole('textbox', { name: 'タスクメモに追記' });
const recorded = (page: Page) => page.evaluate((k) => (JSON.parse(window.sessionStorage.getItem(k) ?? '{"recorded":{}}') as { recorded: Record<string, boolean> }).recorded, STORAGE_KEY);
/** JST の今日 (YYYY-MM-DD) */
const jstToday = () => new Date(Date.now() + 9 * 3600_000).toISOString().slice(0, 10);

test.describe('CRM 架電画面 (実データの表示、API は架空の応答): 保存の守り', () => {
  // 架電結果の入力欄を確かめるので、「架電結果の入力」を前に出した配置で始める (既定は「活動ログ」が前)
  test.beforeEach(async ({ page }) => { await startWithResultPanelInFront(page); await login(page); });

  test('一覧の読み直し中・失敗時は保存しない。一覧が表示されれば項目を 1 回の PATCH で保存して次へ', async ({ page }) => {
    const m = await mockCrm(page);
    await page.goto('/app/crm');
    // 実データでは架空サンプルの帯は出ない
    await expect(page.getByTestId('fixture-banner')).toHaveCount(0);
    await expect(list(page).locator('.cq-row-company')).toHaveText(['架空会社1', '架空会社2']);
    await list(page).getByText('架空会社1').click();

    // 所属チームが分からないので、取引のパイプライン (bpo_リクロジ) で BPO の組になる
    await expect(form(page)).toHaveAttribute('data-team', 'bpo');
    await expect(memo(page)).toBeVisible();
    // 架電日が空なので今日が入っている
    await expect(form(page).getByLabel('架電日', { exact: true })).toHaveValue(jstToday().replaceAll('-', '/'));
    await expect(recordBtn(page)).toHaveAttribute('aria-disabled', 'false');

    // 条件を変えて一覧を読み直している間は保存できない
    await openQueueToolbar(page);
    await page.getByRole('checkbox', { name: '次回日が来たものだけ' }).check();
    await expect(page.getByText('読み込み中…', { exact: true })).toBeVisible();
    await expect.poll(() => m.held.length).toBe(1);
    await expect(recordBtn(page)).toHaveAttribute('aria-disabled', 'true');
    await expect(form(page).locator('.rf-notice')).toHaveText('一覧を読み込み中です。一覧が表示されてから記録してください。');
    await recordBtn(page).click({ force: true });
    await memo(page).press('Control+Enter');
    await expect(page.getByTestId('result-slot')).toHaveAttribute('data-deal-id', '1');
    expect(m.patches).toEqual([]);
    expect(await recorded(page)).toEqual({});

    // 読み直しが失敗しても保存できない
    await m.held[0]?.fulfill({ status: 502, json: { error: 'x', error_kind: 'hubspot_upstream' } });
    const listCol = page.getByRole('region', { name: '架電先の一覧' });
    await expect(listCol.getByRole('alert')).toContainText('取得できませんでしたHubSpot との通信に失敗しました。再試行してください。');
    await expect(form(page).locator('.rf-notice')).toHaveText('一覧を表示できていないため記録できません。一覧を表示してから記録してください。');
    await recordBtn(page).click({ force: true });
    expect(m.patches).toEqual([]);

    // 一覧が表示されれば (案件 1 が載っている) 保存でき、次の案件へ進む
    await listCol.getByRole('button', { name: '再試行' }).click();
    await expect.poll(() => m.held.length).toBe(2);
    const second = m.held[1];
    if (!second) throw new Error('no second request');
    await second.fulfill({ json: queueBody(new URL(second.request().url()), ['1', '2']) });
    await expect(list(page).locator('.cq-row-company')).toHaveText(['架空会社1', '架空会社2']);
    await expect(recordBtn(page)).toHaveAttribute('aria-disabled', 'false');
    await form(page).getByRole('button', { name: '不在', exact: true }).click();
    await memo(page).fill('受付で不在(E2E)');
    await recordBtn(page).click();
    await expect(page.getByTestId('result-slot')).toHaveAttribute('data-deal-id', '2');
    await expect(list(page).locator('li.cq-row').nth(0).locator('.cq-recorded')).toContainText('記録済み(HubSpot に保存済み)');
    expect(await recorded(page)).toEqual({ 'live:1': true });

    // 送ったのは 1 回の PATCH: ステージと項目 (架電日 = 今日、タスクメモ = 日付付きの追記) を一緒に
    const today = jstToday();
    expect(m.patches.map(p => p.path)).toEqual(['/api/crm/deals/1']);
    expect(m.patches[0]?.body.stage).toEqual({ pipeline_id: BPO_PIPELINE, stage_id: '1095387445' });
    expect(m.patches[0]?.body.set).toEqual({ risuto_kadennbi: today, bpo_16: `${today.slice(5, 7)}/${today.slice(8, 10)} 受付で不在(E2E)` });
  });
});
