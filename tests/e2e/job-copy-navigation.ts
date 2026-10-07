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
  await page.getByRole('tablist', { name: '求人管理の機能', exact: true }).getByRole('tab', { name: group, exact: true }).click();
  if (singleFeatureGroups.has(group)) return;
  await page.getByRole('tablist', { name: `${group}の表示`, exact: true }).getByRole('tab', { name: label, exact: true }).click();
}

/** The panel of one function (a single-function group's panel is named by its group tab, so look it up by id). */
export function jobFeaturePanel(page: Page, key: JobFeature) {
  return page.locator(`[role="tabpanel"][id$="-panel-${key}"]`);
}
