import { fireEvent, screen, within } from '@testing-library/react';

/** 「この求人でやること」の札「くわしい分析を開く」から横の画面を開き、feature のタブ（例: タイムライン）に切り替える。 */
export function openOtherAnalysis(feature: string): HTMLElement {
  if (!screen.queryByRole('dialog', { name: 'くわしい分析' })) {
    const section = screen.getByRole('region', { name: 'この求人でやること' });
    fireEvent.click(within(section).getByRole('button', { name: /^くわしい分析を開く/ }));
  }
  const dialog = screen.getByRole('dialog', { name: 'くわしい分析' });
  const tab = within(dialog).getByRole('tab', { name: feature });
  if (tab.getAttribute('aria-selected') !== 'true') fireEvent.click(tab);
  return dialog;
}
