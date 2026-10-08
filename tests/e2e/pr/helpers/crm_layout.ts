import type { Page } from '@playwright/test';

/**
 * 架電画面のパネルの配置 (frontend/src/screens/crm/dockModel.ts) を、このブラウザに残した形で先に入れる。
 *
 * 既定の配置 (v2) では中央の列は「活動ログ」が前、右の列は「求人検索・リンク先」。架電結果の入力欄そのものを確かめるテストは、
 * 「架電結果の入力」を前に出した配置で始める (利用者がタブを押して前に出し、その配置が残った状態と同じ)。
 * 既に配置が残っていれば上書きしない (再読み込みでテストの中の配置を消さない)。
 */
export const DOCK_STORAGE_KEY = 'hrhr.crm.dockLayout.v2';

export const RESULT_FRONT_LAYOUT = JSON.stringify({
  v: 2,
  columns: [
    { panels: ['queue', 'properties'], active: 'queue' },
    { panels: ['overview', 'activity', 'result'], active: 'result' },
    { panels: ['links'], active: 'links' },
  ],
  widths: [0.21, 0.34, 0.45],
});

export async function startWithResultPanelInFront(page: Page): Promise<void> {
  await page.addInitScript(([key, value]) => {
    try { if (window.localStorage.getItem(key) === null) window.localStorage.setItem(key, value); } catch { /* 残せない環境 */ }
  }, [DOCK_STORAGE_KEY, RESULT_FRONT_LAYOUT] as const);
}
