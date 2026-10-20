import { createElement } from 'react';
import type { PreviewScreen } from '../../preview';
import { ColumnsScreen } from './ColumnsScreen';

export const screens: PreviewScreen[] = [
  { id: 'columns', group: '顧客（ログイン後）', title: '項目の設定', render: () => createElement(ColumnsScreen) },
];
