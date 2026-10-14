import { fireEvent, screen, within } from '@testing-library/react';

/** 「ほかの分析」の節から、横から出る分析の画面を開く。feature は押すボタンの表示名（例: タイムライン）。 */
export function openOtherAnalysis(feature: string): HTMLElement {
  const section = screen.getByRole('region', { name: 'ほかの分析' });
  fireEvent.click(within(section).getByRole('button', { name: feature }));
  return screen.getByRole('dialog', { name: 'ほかの分析' });
}
