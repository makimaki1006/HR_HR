import { useState } from 'react';
import { Button, Notice } from '../../ui/kit';
import { SERVICE_ACCOUNT_EMAIL } from './state';

/** シートが共有されていないときの案内。共有先のメールアドレスをコピーできる。 */
export function ShareHint({ onRecheck, checking }: { onRecheck?: () => void; checking?: boolean }) {
  const [copied, setCopied] = useState(false);
  const copy = () => {
    void navigator.clipboard.writeText(SERVICE_ACCOUNT_EMAIL).catch(() => undefined);
    setCopied(true);
  };
  return (
    <Notice tone="warn" title="シートが共有されていません">
      <p>シートの「共有」で、次のメールアドレスを編集者として追加してから、もう一度確認してください。</p>
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <code className="rounded border border-pt-line bg-pt-surface px-2 py-1 text-sm">{SERVICE_ACCOUNT_EMAIL}</code>
        <Button size="sm" onClick={copy}>{copied ? 'コピーしました' : 'コピー'}</Button>
        {onRecheck && <Button size="sm" variant="primary" disabled={checking} onClick={onRecheck}>{checking ? '確認しています' : 'つながりを確かめ直す'}</Button>}
      </div>
    </Notice>
  );
}
