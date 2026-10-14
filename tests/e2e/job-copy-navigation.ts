import type { Page } from '@playwright/test';

export type JobFeature = 'timeline' | 'body' | 'applications' | 'applicants' | 'reasons' | 'performance' | 'market' | 'market-table' | 'factors' | 'diff' | 'ab' | 'report' | 'receive';
export const jobFeatures: Record<JobFeature, { group: string; label: string }> = {
  timeline: { group: 'タイムライン', label: 'タイムライン' },
  body: { group: '求人内容', label: '本文・画像' },
  applications: { group: '応募分析', label: '応募推移' },
  applicants: { group: '応募分析', label: '応募者構成' },
  reasons: { group: '応募分析', label: '応募理由' },
  performance: { group: '応募分析', label: '課金・クリック' },
  market: { group: '市場分析', label: '市場グラフ' },
  'market-table': { group: '市場分析', label: '市場データ' },
  factors: { group: '市場分析', label: '要因・仮説' },
  diff: { group: '比較・報告', label: '変更差分' },
  ab: { group: '比較・報告', label: '2求人のA/B比較' },
  report: { group: '比較・報告', label: '顧客報告・検証' },
  receive: { group: 'データ取込', label: '外部文面を確認' },
};

/** Groups with one function show no second row of tabs (2026-10-08). */
const singleFeatureGroups = new Set(['タイムライン', '求人内容']);

/**
 * Select the primary function first so a previously remembered leaf is safe.
 * 外部文面を確認 is no longer a tab: it opens from the データ取込 area at the top of the page.
 */
export async function selectJobFeature(page: Page, key: JobFeature) {
  const { group, label } = jobFeatures[key];
  if (key === 'receive') {
    const toggle = page.getByRole('button', { name: 'データ取込', exact: true });
    if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
    await page.getByRole('button', { name: '外部文面を照合する', exact: true }).click();
    return;
  }
  // 旧タブは「ほかの分析」の横画面の中にある。開いていなければ、機能のボタンから開く（その機能が選ばれた状態で開く）。
  const tablist = page.getByRole('tablist', { name: '求人管理の機能', exact: true });
  if (!await tablist.isVisible()) {
    await openOtherAnalysis(page, label);
    return;
  }
  await tablist.getByRole('tab', { name: group, exact: true }).click();
  if (singleFeatureGroups.has(group)) return;
  await page.getByRole('tablist', { name: `${group}の表示`, exact: true }).getByRole('tab', { name: label, exact: true }).click();
}

/** The panel of one function (a single-function group's panel is named by its group tab, so look it up by id). */
export function jobFeaturePanel(page: Page, key: JobFeature) {
  return page.locator(`[role="tabpanel"][id$="-panel-${key}"]`);
}

/** 「ほかの分析」の機能ボタン（ページの本体にある）から、横に出る詳細を開く。ボタンの文字は機能の名前。 */
export async function openOtherAnalysis(page: Page, label = 'タイムライン') {
  await page.getByRole('region', { name: 'ほかの分析', exact: true }).getByRole('button', { name: label, exact: true }).click();
  await page.getByRole('dialog', { name: 'ほかの分析', exact: true }).waitFor({ state: 'visible' });
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
  if (await button.isVisible()) await button.click();
}
