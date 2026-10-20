import { useState } from 'react';
import { useData } from '../../data';
import { Button } from '../../ui/kit';
import type { AccountStatus, PortalAccount } from '../../types';
import { StatusDot } from '../customers/shared';
import { isInviteExpired } from './store';

export const STATUS_LABEL: Record<AccountStatus, string> = { invited: '招待中', active: '利用中', locked: 'ロック中', disabled: '停止中' };

const TONE: Record<AccountStatus, 'ok' | 'warn' | 'neutral'> = { active: 'ok', invited: 'warn', locked: 'warn', disabled: 'neutral' };

export function AccountStatusBadge({ account }: { account: PortalAccount }) {
  const d = useData();
  if (d.customers.find((c) => c.id === account.customerId)?.disabled === true) return <StatusDot tone="neutral">顧客ごと停止中</StatusDot>;
  if (isInviteExpired(account)) return <StatusDot tone="danger">招待の期限切れ</StatusDot>;
  return <StatusDot tone={TONE[account.status]}>{STATUS_LABEL[account.status]}</StatusDot>;
}

export function CopyButton({ text, label = 'コピー', primary = false }: { text: string; label?: string; primary?: boolean }) {
  const [done, setDone] = useState(false);
  const copy = (): void => {
    try {
      void navigator.clipboard.writeText(text).catch(() => undefined);
    } catch {
      /* 試作では失敗しても表示だけ変える */
    }
    setDone(true);
    window.setTimeout(() => { setDone(false); }, 2000);
  };
  return <Button size={primary ? 'md' : 'sm'} variant={primary ? 'primary' : 'secondary'} onClick={copy}>{done ? 'コピーした' : label}</Button>;
}
