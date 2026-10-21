import { createElement } from 'react';
import type { PreviewScreen } from '../../preview';
import { Code } from './Code';
import { Invite } from './Invite';
import { Login } from './Login';
import { Forgot, InviteExpired, LoggedOut, Locked } from './Simple';
import { Welcome } from './Welcome';

const G = '顧客（ログイン前）' as const;

export const screens: PreviewScreen[] = [
  { id: 'auth-login', group: G, title: 'ログイン', render: () => createElement(Login) },
  { id: 'auth-code', group: G, title: '確認コード（新しい端末）', render: () => createElement(Code) },
  { id: 'auth-locked', group: G, title: 'ロック中', render: () => createElement(Locked) },
  { id: 'auth-forgot', group: G, title: 'パスワードを忘れたとき', render: () => createElement(Forgot) },
  { id: 'auth-invite', group: G, title: '招待からのパスワード設定', render: () => createElement(Invite) },
  { id: 'auth-invite-expired', group: G, title: '招待リンクの期限切れ・使用済み', render: () => createElement(InviteExpired) },
  { id: 'auth-welcome', group: G, title: 'はじめての案内', render: () => createElement(Welcome) },
  { id: 'auth-logged-out', group: G, title: 'ログアウトしました', render: () => createElement(LoggedOut) },
];
