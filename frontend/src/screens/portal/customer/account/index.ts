import { createElement } from 'react';
import type { PreviewScreen } from '../../preview';
import { AccountScreen } from './AccountScreen';

export const screens: PreviewScreen[] = [
  { id: 'account', group: '顧客（ログイン後）', title: 'アカウントの設定', render: () => createElement(AccountScreen) },
  { id: 'account-password-done', group: '顧客（ログイン後）', title: 'パスワードを変更したあと', render: () => createElement(AccountScreen, { passwordDone: true }) },
  { id: 'account-save-failed', group: '顧客（ログイン後）', title: '設定を保存できなかったとき', render: () => createElement(AccountScreen, { failing: true }) },
];
