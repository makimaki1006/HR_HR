import { useState } from 'react';
import { Button } from '../../ui/kit';
import { SERVICE_ACCOUNT_EMAIL } from './state';

/** シートが共有されていないときの案内。共有先のメールアドレスをコピーできる。 */
export function ShareHint({ onRecheck, checking }: { onRecheck?: () => void; checking?: boolean }) {
  const [copied, setCopied] = useState(false);
  const copy = (): void => {
    try {
      void navigator.clipboard.writeText(SERVICE_ACCOUNT_EMAIL).catch(() => undefined);
    } catch {
      /* 試作では表示だけ変える */
    }
    setCopied(true);
  };
  return (
    <div className="rounded-md border border-pt-line bg-pt-sunken px-3 py-2.5 text-[13px]">
      <p className="font-medium"><span className="mr-1.5 inline-block h-1.5 w-1.5 rounded-full bg-pt-warn align-middle" />シートが共有されていない</p>
      <p className="mt-0.5 text-xs text-pt-muted">シートの「共有」で次のアドレスを編集者に追加してから、確かめ直す。</p>
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <code className="rounded border border-pt-line bg-pt-surface px-2 py-1 text-xs">{SERVICE_ACCOUNT_EMAIL}</code>
        <Button size="sm" onClick={copy}>{copied ? 'コピーした' : 'コピー'}</Button>
        {onRecheck && <Button size="sm" variant="primary" disabled={checking} onClick={onRecheck}>{checking ? '確認中' : '確かめ直す'}</Button>}
      </div>
    </div>
  );
}
