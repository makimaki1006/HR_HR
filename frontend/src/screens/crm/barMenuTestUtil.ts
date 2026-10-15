import { fireEvent, screen } from '@testing-library/react';

/** 条件を変更する操作欄を固定して開く */
export function openQueueToolbar() {
  const toggle = screen.getByRole('button', { name: /^条件を変更:/ });
  if (toggle.getAttribute('aria-pressed') !== 'true') fireEvent.click(toggle);
}

/** 絞り込みバー右端の「…」メニューを開く */
export function openBarMenu(): HTMLElement {
  openQueueToolbar();
  fireEvent.click(screen.getByRole('button', { name: 'その他の操作' }));
  return screen.getByRole('menu', { name: 'その他の操作' });
}

/** 「…」メニューの項目を押す */
export function clickBarMenuItem(name: string) {
  openBarMenu();
  fireEvent.click(screen.getByRole('menuitem', { name }));
}

/** 実データ → 架空サンプル (メニューから) */
export const toFixture = () => { clickBarMenuItem('架空サンプルを表示'); };
/** 架空サンプル → 実データ (上の帯のリンクから) */
export const toLive = () => { fireEvent.click(screen.getByRole('button', { name: '実データに戻す' })); };
