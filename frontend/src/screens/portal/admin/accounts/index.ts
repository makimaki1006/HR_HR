import { createElement } from 'react';
import type { PreviewScreen } from '../../preview';
import { AccountDetail } from './AccountDetail';
import { AccountList } from './AccountList';
import { Activity } from './Activity';
import { Invite } from './Invite';

export const screens: PreviewScreen[] = [
  { id: 'admin-accounts', group: '社内の管理', title: 'アカウントの一覧', render: () => createElement(AccountList) },
  { id: 'admin-invite', group: '社内の管理', title: '招待する', render: () => createElement(Invite) },
  { id: 'admin-account-detail', group: '社内の管理', title: 'アカウントの詳細', render: () => createElement(AccountDetail) },
  { id: 'admin-activity', group: '社内の管理', title: '操作の記録', render: () => createElement(Activity) },
];
