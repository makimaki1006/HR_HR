import { createElement } from 'react';
import type { PreviewScreen } from '../../preview';
import { EditScreen } from './EditScreen';

export const screens: PreviewScreen[] = [
  { id: 'edit', group: '顧客（ログイン後）', title: '応募者の詳細と編集', render: () => createElement(EditScreen, { next: 'ok' }) },
  { id: 'edit-conflict', group: '顧客（ログイン後）', title: '編集（シートで先に変更されている）', render: () => createElement(EditScreen, { next: 'conflict' }) },
  { id: 'edit-failed', group: '顧客（ログイン後）', title: '編集（保存できない）', render: () => createElement(EditScreen, { next: 'failed' }) },
];
