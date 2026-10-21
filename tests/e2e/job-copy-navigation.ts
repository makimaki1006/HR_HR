import type { Page } from '@playwright/test';

export type JobFeature = 'timeline' | 'body' | 'applications' | 'applicants' | 'reasons' | 'performance' | 'market' | 'market-table' | 'factors' | 'diff' | 'ab' | 'report' | 'receive';
export const jobFeatures: Record<JobFeature, { group: string; label: string }> = {
  timeline: { group: 'タイムライン', label: 'タイムライン' },
  body: { group: '本文・画像', label: '本文・画像' },
  applications: { group: '応募推移', label: '応募推移' },
  applicants: { group: '応募者構成', label: '応募者構成' },
  reasons: { group: '応募理由', label: '応募理由' },
  performance: { group: '課金・クリック', label: '課金・クリック' },
  market: { group: '市場グラフ', label: '市場グラフ' },
  'market-table': { group: '市場データ', label: '市場データ' },
  factors: { group: '要因・仮説', label: '要因・仮説' },
  diff: { group: '変更差分', label: '変更差分' },
  ab: { group: '求人比較', label: '求人比較' },
  report: { group: '顧客報告・検証', label: '顧客報告・検証' },
  receive: { group: '外部文面を確認', label: '外部文面を確認' },
};

/** 「ほかの分析」のタブに並ぶ 7 つ。それ以外（応募推移・課金・クリック・市場グラフ・市場データ・顧客報告・検証）は URL の feature で開く。 */
const tabbedFeatures = new Set<JobFeature>(['timeline', 'body', 'diff', 'ab', 'applicants', 'reasons', 'factors']);

/**
 * 求人の復元を待ち、開いているドロワーのタブか、画面本体の機能ボタンから選ぶ。
 * 外部文面の確認はタブではなく、画面上部の「データ取込」から開く。
 */
export async function selectJobFeature(page: Page, key: JobFeature) {
  const { label } = jobFeatures[key];
  if (key === 'receive') {
    const toggle = page.getByRole('button', { name: 'データ取込', exact: true });
    if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
    await page.getByRole('button', { name: '外部文面を照合する', exact: true }).click();
    return;
  }
  await openOtherAnalysis(page, label);
}

/** The panel of one function (a single-function group's panel is named by its group tab, so look it up by id). */
export function jobFeaturePanel(page: Page, key: JobFeature) {
  return page.locator(`[role="tabpanel"][id$="-panel-${key}"]`);
}

/** 「ほかの分析」を指定機能で表示する。入口のボタンで横画面を開き、タブで切り替える。タブに無い機能は URL の feature で開く。 */
export async function openOtherAnalysis(page: Page, label = 'タイムライン') {
  const entry = page.getByRole('region', { name: 'ほかの分析', exact: true });
  const dialog = page.getByRole('dialog', { name: 'ほかの分析', exact: true });
  // 求人の非同期読込を待つ。URL から analysis=1 を復元する場合、背後のボタンは押せない。
  await entry.waitFor({ state: 'visible' });
  const feature = Object.entries(jobFeatures).find(([, item]) => item.label === label);
  if (!feature) throw new Error(`求人の機能が見つかりません: ${label}`);
  const [key] = feature as [JobFeature, { group: string; label: string }];
  if (new URL(page.url()).searchParams.get('analysis') !== '1' && !await dialog.isVisible()) {
    await entry.getByRole('button', { name: 'ほかの分析を開く', exact: true }).click();
  }
  await dialog.waitFor({ state: 'visible' });
  if (!tabbedFeatures.has(key)) {
    await page.evaluate(feature => {
      const url = new URL(window.location.href);
      url.searchParams.set('feature', feature);
      window.history.pushState(window.history.state, '', url);
      window.dispatchEvent(new PopStateEvent('popstate'));
    }, key);
    await dialog.getByRole('heading', { name: label, exact: true }).waitFor({ state: 'visible' });
    return;
  }
  const tab = dialog.getByRole('tablist', { name: '求人管理の機能', exact: true }).getByRole('tab', { name: label, exact: true });
  if (await tab.getAttribute('aria-selected') !== 'true') await tab.click();
}

/** 「ほかの分析」の横画面を閉じる（閉じると元の位置に戻り、ページが操作できるようになる）。 */
export async function closeOtherAnalysis(page: Page) {
  const dialog = page.getByRole('dialog', { name: 'ほかの分析', exact: true });
  await dialog.getByRole('button', { name: '閉じる', exact: true }).first().click();
  await dialog.waitFor({ state: 'hidden' });
}

/** 左の一覧は「求人を探す」を押すまで求人を読まない（案件管理 2026-10-15）。押して、求人の一覧を開く。 */
export async function findListings(page: Page) {
  await page.locator('.jc-sidebar').hover();
  const button = page.getByRole('button', { name: '求人を探す', exact: true });
  const open = page.getByRole('button', { name: '求人の検索を閉じる', exact: true });
  // マウスで開く側は展開に時間がかかる。すでに開いている・ボタンが無い画面（固定一覧など）では待たずに抜ける。
  await button.or(open).first().waitFor({ state: 'visible', timeout: 2_000 }).catch(() => undefined);
  if (await button.isVisible()) await button.click();
}
