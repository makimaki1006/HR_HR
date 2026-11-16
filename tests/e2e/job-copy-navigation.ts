import type { Page } from '@playwright/test';

export type JobFeature = 'timeline' | 'body' | 'applications' | 'applicants' | 'reasons' | 'performance' | 'market' | 'market-table' | 'factors' | 'diff' | 'ab' | 'report' | 'receive';
export const jobFeatures: Record<JobFeature, { group: string; label: string }> = {
  timeline: { group: 'タイムライン', label: 'タイムライン' },
  body: { group: '本文と画像', label: '本文と画像' },
  applications: { group: '応募推移', label: '応募推移' },
  applicants: { group: '応募者', label: '応募者' },
  reasons: { group: '応募理由', label: '応募理由' },
  performance: { group: '課金・クリック', label: '課金・クリック' },
  market: { group: '市場グラフ', label: '市場グラフ' },
  'market-table': { group: '市場データ', label: '市場データ' },
  factors: { group: 'メモ', label: 'メモ' },
  diff: { group: '変更の前後', label: '変更の前後' },
  ab: { group: 'ほかの求人と比べる', label: 'ほかの求人と比べる' },
  report: { group: '検証記録', label: '検証記録' },
  receive: { group: '外部文面を確認', label: '外部文面を確認' },
};

/** 「くわしい分析」のタブに並ぶ 6 つ。それ以外（タイムライン・応募推移・課金・クリック・市場グラフ・市場データ・検証記録）は URL の feature で開く。 */
const tabbedFeatures = new Set<JobFeature>(['body', 'diff', 'ab', 'applicants', 'reasons', 'factors']);

/**
 * 求人の復元を待ち、開いているドロワーのタブか、画面本体の機能ボタンから選ぶ。
 * 外部文面の確認はタブではなく、画面上部の「⋯ その他」→「データを取り込む（作業用）」から開く。
 */
export async function selectJobFeature(page: Page, key: JobFeature) {
  const { label } = jobFeatures[key];
  if (key === 'receive') {
    await openDataImport(page);
    await page.getByRole('button', { name: '外部文面を照合する', exact: true }).click();
    return;
  }
  await openOtherAnalysis(page, label);
}

/** The panel of one function (a single-function group's panel is named by its group tab, so look it up by id). */
export function jobFeaturePanel(page: Page, key: JobFeature) {
  return page.locator(`[role="tabpanel"][id$="-panel-${key}"]`);
}

/**
 * 求人の画面の「この求人でやること」の札を開く（2026-11-15 まで一番下の畳んだ「作業」だったもの）。
 * task: 文面を手直しする / リクロジ向けの案を作る / 電話ヒアリングを見る。すでに開いていれば何もしない。
 */
export async function openWork(page: Page, task: '文面を手直しする' | 'リクロジ向けの案を作る' | '電話ヒアリングを見る' = '文面を手直しする') {
  const card = page.locator('.lt-card', { hasText: task });
  await card.waitFor({ state: 'visible' });
  if (await card.getAttribute('aria-pressed') !== 'true') await card.click();
  await page.getByRole('region', { name: task, exact: true }).waitFor({ state: 'visible' });
}

/** 「くわしい分析」を指定機能で表示する。「この求人でやること」の札で横画面を開き、タブで切り替える。タブに無い機能は URL の feature で開く。 */
export async function openOtherAnalysis(page: Page, label = 'タイムライン') {
  const dialog = page.getByRole('dialog', { name: 'くわしい分析', exact: true });
  const card = page.locator('.lt-card', { hasText: 'くわしい分析を開く' });
  // 求人の非同期読込を待つ。URL から analysis=1 を復元する場合や、すでにドロワーが開いている場合は、背後の札は押せない。
  await dialog.or(card).first().waitFor({ state: 'visible' });
  const feature = Object.entries(jobFeatures).find(([, item]) => item.label === label);
  if (!feature) throw new Error(`求人の機能が見つかりません: ${label}`);
  const [key] = feature as [JobFeature, { group: string; label: string }];
  if (new URL(page.url()).searchParams.get('analysis') !== '1' && !await dialog.isVisible()) await card.click();
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

/** 「くわしい分析」の横画面を閉じる（閉じると元の位置に戻り、ページが操作できるようになる）。 */
export async function closeOtherAnalysis(page: Page) {
  const dialog = page.getByRole('dialog', { name: 'くわしい分析', exact: true });
  await dialog.getByRole('button', { name: '閉じる', exact: true }).first().click();
  await dialog.waitFor({ state: 'hidden' });
}

/** 上の帯の「⋯ その他」からデータ取込を開く。すでに開いていれば何もしない。 */
export async function openDataImport(page: Page) {
  if (await page.getByRole('region', { name: 'データ取込', exact: true }).isVisible()) return;
  await page.getByRole('button', { name: 'その他', exact: true }).click();
  await page.getByRole('button', { name: 'データを取り込む（作業用）', exact: true }).click();
}

/** 左の一覧は「求人を探す」を押すまで求人を読まない（案件管理 2026-10-15）。押して、求人の一覧を開く。 */
export async function findListings(page: Page) {
  // 左の帯は押したときだけ開く（マウスが通るだけでは開かない。2026-11-15）。
  const trigger = page.getByRole('button', { name: '左の一覧で求人を探す', exact: true });
  if (await trigger.isVisible() && await trigger.getAttribute('aria-expanded') !== 'true') await trigger.click();
  const button = page.getByRole('button', { name: '求人を探す', exact: true });
  const open = page.getByRole('button', { name: '求人の検索を閉じる', exact: true });
  // マウスで開く側は展開に時間がかかる。すでに開いている・ボタンが無い画面（固定一覧など）では待たずに抜ける。
  await button.or(open).first().waitFor({ state: 'visible', timeout: 2_000 }).catch(() => undefined);
  if (await button.isVisible()) await button.click();
}
