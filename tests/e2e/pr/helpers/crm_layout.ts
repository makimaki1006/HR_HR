import type { Page } from '@playwright/test';

/**
 * 架電画面のパネルの配置 (frontend/src/screens/crm/dockModel.ts) を、このブラウザに残した形で先に入れる。
 *
 * 既定の配置では中央の列は「活動ログ」が前に出ている。架電結果の入力欄そのものを確かめるテストは、
 * 「架電結果の入力」を前に出した配置で始める (利用者がタブを押して前に出し、その配置が残った状態と同じ)。
 * 既に配置が残っていれば上書きしない (再読み込みでテストの中の配置を消さない)。
 */
export const DOCK_STORAGE_KEY = 'hrhr.crm.dockLayout.v1';

export const RESULT_FRONT_LAYOUT = JSON.stringify({
  v: 1,
  columns: [
    { panels: ['queue', 'properties'], active: 'queue' },
    { panels: ['overview', 'activity', 'result', 'links'], active: 'result' },
    { panels: [], active: null },
  ],
  widths: [0.26, 0.74, 0.3],
});

export async function startWithResultPanelInFront(page: Page): Promise<void> {
  await page.addInitScript(([key, value]) => {
    try { if (window.localStorage.getItem(key) === null) window.localStorage.setItem(key, value); } catch { /* 残せない環境 */ }
  }, [DOCK_STORAGE_KEY, RESULT_FRONT_LAYOUT] as const);
}
