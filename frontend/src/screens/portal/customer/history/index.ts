import { createElement } from 'react';
import type { PreviewScreen } from '../../preview';
import { History } from './History';

export const screens: PreviewScreen[] = [
  { id: 'history', group: '顧客（ログイン後）', title: '記録', render: () => createElement(History) },
];
