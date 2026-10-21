import { fireEvent, screen, within } from '@testing-library/react';

/** 「ほかの分析」の節の入口から横の画面を開き、feature のタブ（例: タイムライン）に切り替える。 */
export function openOtherAnalysis(feature: string): HTMLElement {
  if (!screen.queryByRole('dialog', { name: 'ほかの分析' })) {
    const section = screen.getByRole('region', { name: 'ほかの分析' });
    fireEvent.click(within(section).getByRole('button', { name: 'ほかの分析を開く' }));
  }
  const dialog = screen.getByRole('dialog', { name: 'ほかの分析' });
  const tab = within(dialog).getByRole('tab', { name: feature });
  if (tab.getAttribute('aria-selected') !== 'true') fireEvent.click(tab);
  return dialog;
}
