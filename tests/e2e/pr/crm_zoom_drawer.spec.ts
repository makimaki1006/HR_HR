import { expect, Page } from '@playwright/test';
import { test } from '@playwright/test';
import { login } from './helpers/login';
import { QUEUE_STAGE_IDS } from '../../../frontend/src/screens/crm/queueModel';

/**
 * 架電画面 (/app/crm、実データの表示) の Zoom の枠 (右から開く引き出し) の E2E。
 *
 * - /api/crm/* は page.route で架空の応答を返す (HubSpot へは接続しない)。
 * - Zoom の枠 (https://applications.zoom.us/…) は、同じ URL に架空のページを返す。
 *   架空のページは zp-make-call を受けたら記録し、呼び出し中のイベントを親へ返す (本物の Zoom は読み込まない)。
 * - 値は画面の文字・要素の位置・iframe の中の記録で確かめる。この画面にグラフ (ECharts) は無い。
 */

const STUB_ZOOM = `<!doctype html><meta charset="utf-8"><title>stub</title><body>架空の Zoom<script>
window.__got = [];
window.addEventListener('message', function (e) {
  var d = e.data;
  if (!d || d.type !== 'zp-make-call') return;
  window.__got.push(d.data.number);
  parent.postMessage({ type: 'zp-call-ringing-event', data: { callId: 'e2e-1', direction: 'outbound', callee: { phoneNumber: d.data.number } } }, '*');
});
</script>`;

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

function queueBody(url: URL) {
  const p = url.searchParams;
  return {
    items: ['1', '2'].map(item), next_cursor: null, total: 2, truncated: false,
    scope: {
      pipeline: '753186575', owner: 'all', role: 'admin', teams: [], stages: [...QUEUE_STAGE_IDS].sort(),
      due: p.get('due') ?? 'all', sort: p.get('sort') ?? 'default', q: null, limit: Number(p.get('limit') ?? 25),
      next_from: null, next_to: null, last_from: null, last_to: null,
    },
    partial: { missing_contacts: 0, missing_companies: 0, failed: [], excluded: { no_phone: 0, stop_reason: 0, out_of_scope: 0 }, unknown_stages: 0 },
    generated_at: '2026-10-08T03:00:00Z',
  };
}

function detailBody(id: string) {
  return {
    deal: {
      id, name: `架空案件${id}`, stage_id: '1095387442', stage_label: '未済', pipeline_id: '753186575', owner_id: null,
      amount: null, close_date: null, next_call_date: null, next_call_time: null, last_call_date: null,
      stop: { prohibited_reason: null, block_reason: null, unreachable_check: null }, bpo_phone: null,
      deep_link: `https://app.hubspot.com/contacts/1/record/0-3/${id}/`,
    },
    dial: { number: '+81300000001', source: 'contact' },
    contacts: [], contacts_total: 0,
    companies: [{ id: `co${id}`, name: `架空会社${id}`, phone: null, address: null, industry: null, domain: null, labels: ['主'], is_primary: true, deep_link: `https://app.hubspot.com/contacts/1/record/0-2/co${id}/` }],
    companies_total: 1,
    activities: [], activities_truncated: false, activity_scope: '架空', partial: [], hubspot_portal_id: '1', data_scope: 'x', generated_at: '2026-10-08T03:00:00Z',
  };
}

async function mockCrm(page: Page): Promise<{ writes: string[] }> {
  const m = { writes: [] as string[] };
  await page.route(/applications\.zoom\.us/, (r) => r.fulfill({ contentType: 'text/html; charset=utf-8', body: STUB_ZOOM }));
  await page.route('**/api/crm/**', async (route) => {
    const req = route.request();
    const url = new URL(req.url());
    if (req.method() !== 'GET') m.writes.push(`${req.method()} ${url.pathname}`);
    if (url.pathname === '/api/crm/call-queue') { await route.fulfill({ json: queueBody(url) }); return; }
    const d = /^\/api\/crm\/workspace\/deals\/(\w+)$/.exec(url.pathname);
    if (d?.[1]) { await route.fulfill({ json: detailBody(d[1]) }); return; }
    await route.fulfill({ status: 502, json: { error: 'x', error_kind: 'hubspot_upstream' } });
  });
  return m;
}

const drawer = (page: Page) => page.getByTestId('zoom-drawer');
const toggle = (page: Page) => page.getByTestId('zoom-toggle');

test.describe('CRM 架電画面: Zoom の枠 (引き出し)', () => {
  test.beforeEach(async ({ page }) => { await login(page); });

  test('閉じた状態で始まり中央が広がる。閉じたまま発信が Zoom に届き、呼び出し中が番号の下に出る。開け閉めで枠は作り直さない', async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    const m = await mockCrm(page);
    await page.goto('/app/crm');
    await expect(page.getByRole('status', { name: 'データの種類' })).toContainText('実データ(HubSpot)');
    const list = page.getByRole('list', { name: '架電キュー' });
    await list.getByText('架空会社1').click();
    await expect(page.getByRole('article', { name: '架電先の詳細' }).locator('h2')).toHaveText('架空会社1');

    // 閉じている: 枠は画面の外 (同じ幅のまま)、中央の列は右端まで広がる
    await expect(toggle(page)).toHaveAttribute('aria-expanded', 'false');
    await expect(drawer(page)).toHaveAttribute('aria-hidden', 'true');
    const closed = await page.evaluate(() => {
      const d = document.querySelector('[data-testid="zoom-drawer"]')?.getBoundingClientRect();
      const f = document.querySelector('iframe[title="Zoom Phone"]')?.getBoundingClientRect();
      const c = document.querySelector('.cq-detail')?.getBoundingClientRect();
      return { drawerLeft: d?.left ?? 0, frameWidth: f?.width ?? 0, frameHeight: f?.height ?? 0, centerRight: c?.right ?? 0, vw: window.innerWidth, sw: document.documentElement.scrollWidth };
    });
    expect(closed.drawerLeft).toBeGreaterThanOrEqual(closed.vw);
    expect(closed.frameWidth).toBeGreaterThan(300);
    expect(closed.frameHeight).toBeGreaterThan(400);
    expect(closed.centerRight).toBe(closed.vw);
    // 画面の外に置いた枠で横スクロールが出ない
    expect(closed.sw).toBe(closed.vw);

    // 閉じたまま発信 (枠は読み込み済みだが、まだ何も言ってこないので枠は自動で開く)
    const frame = page.frameLocator('iframe[title="Zoom Phone"]');
    await expect(frame.locator('body')).toContainText('架空の Zoom');
    const iframeHandle = await page.locator('iframe[title="Zoom Phone"]').elementHandle();
    await page.getByRole('button', { name: /に発信$/ }).first().click();
    await expect(page.getByTestId('call-bar-status')).toHaveText('呼び出し中');
    const got = await page.frames().find((f) => f.url().includes('applications.zoom.us'))?.evaluate(() => (window as unknown as { __got: string[] }).__got);
    expect(got).toEqual(['+81300000001']);
    await expect(toggle(page)).toContainText('呼び出し中');
    // 一度も応答していない枠への発信なので開いている (サインインの確認のため)。Esc で閉じ、フォーカスは「Zoom」ボタンへ
    await expect(drawer(page)).toHaveClass(/is-open/);
    // 開く動き (0.18 秒) が終わってから、右端に 360px で重なっていること
    await expect.poll(async () => {
      const b = await drawer(page).boundingBox();
      return [Math.round(b?.width ?? 0), Math.round((b?.x ?? 0) + (b?.width ?? 0))];
    }).toEqual([360, 1440]);
    await drawer(page).getByRole('button', { name: 'Zoom の枠を閉じる' }).focus();
    await page.keyboard.press('Escape');
    await expect(drawer(page)).not.toHaveClass(/is-open/);
    await expect(toggle(page)).toBeFocused();

    // 開け閉め・案件の切り替えで同じ iframe のまま
    await toggle(page).click();
    await expect(drawer(page).getByRole('button', { name: 'Zoom の枠を閉じる' })).toBeFocused();
    await toggle(page).click();
    await list.getByText('架空会社2').click();
    await expect(page.getByRole('article', { name: '架電先の詳細' }).locator('h2')).toHaveText('架空会社2');
    const same = await page.locator('iframe[title="Zoom Phone"]').evaluate((el, h) => el === h, iframeHandle);
    expect(same).toBe(true);
    expect(m.writes).toEqual([]);
  });
});
