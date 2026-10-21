import { createElement } from 'react';
import type { PreviewScreen } from '../../preview';
import { CustomerList } from './CustomerList';
import { CustomerDetail } from './CustomerDetail';
import { SheetAdd } from './SheetAdd';
import { SheetColumns } from './SheetColumns';

export const screens: PreviewScreen[] = [
  { id: 'admin-customers', group: '社内の管理', title: '顧客の一覧', render: () => createElement(CustomerList) },
  { id: 'admin-customer-detail', group: '社内の管理', title: '顧客の詳細', render: () => createElement(CustomerDetail) },
  { id: 'admin-sheet-add', group: '社内の管理', title: 'シートの追加', render: () => createElement(SheetAdd) },
  { id: 'admin-sheet-columns', group: '社内の管理', title: '列の設定', render: () => createElement(SheetColumns) },
];
