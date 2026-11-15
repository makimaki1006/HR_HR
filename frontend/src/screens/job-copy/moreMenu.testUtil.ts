import { fireEvent, screen } from '@testing-library/react';

/** 上の帯の「⋯ その他」を開き、中の項目を押す（テスト用）。 */
export function pressMoreItem(name: string) {
  fireEvent.click(screen.getByRole('button', { name: 'その他' }));
  fireEvent.click(screen.getByRole('button', { name }));
}
/** データ取込を開く・閉じる（「⋯ その他」の中にある）。 */
export function toggleDataImport() { pressMoreItem('データを取り込む（作業用）'); }
