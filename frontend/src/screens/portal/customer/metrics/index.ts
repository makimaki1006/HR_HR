import { createElement } from 'react';
import type { PreviewScreen } from '../../preview';
import { MetricsScreen } from './MetricsScreen';

export const screens: PreviewScreen[] = [
  { id: 'metrics', group: '顧客（ログイン後）', title: '数字', render: () => createElement(MetricsScreen) },
];
