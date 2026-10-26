// 確認コードのメールの送信元 (ADR-036)。送信元の社員本人の Gmail の許可を取る・状態を見る・取り消す。
// 許可を押せるのは送信元に決まっている本人だけ (サーバーが確かめる)。取り消しは管理者なら誰でも。
import { useState } from 'react';
import type { PortalAdminMail } from '../../../generated/PortalAdminMail';
import type { PortalAdminStatus } from '../../../generated/PortalAdminStatus';
import { Button, Card, Notice, formatShortDateTime } from '../ui/kit';
import { API, post } from './api';
import { ErrorLine, LoadError, Loading, Page, useGet } from './parts';

const STATE_LABEL: Record<string, string> = {
  granted: '許可済み',
  none: '未許可',
  expired: '切れている',
  unknown: '確かめられません',
};

function MailBody({ mail, onChanged }: { mail: PortalAdminMail; onChanged: () => void }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!mail.configured) {
    return (
      <Notice tone="warn" title="メールの送信元が設定されていません">
        送信元にする社員を決めて設定すると使えるようになります（設定は開発の担当者に頼んでください）。設定されるまで、顧客はログインできません。
      </Notice>
    );
  }
  const revoke = async (): Promise<void> => {
    setBusy(true);
    setError(null);
    const r = await post<{ state: string; google_revocation_pending: boolean }>(`${API}/mail/revoke`, {});
    setBusy(false);
    if (!r.ok) {
      setError(r.message);
      return;
    }
    if (r.data.google_revocation_pending) {
      setError('この画面では取り消しました。Google 側の取り消しはできなかったので、送信元の人の Google アカウントの設定からも取り消してください。');
    }
    onChanged();
  };
  const granted = mail.state === 'granted';
  return (
    <Card className="p-5">
      <dl className="grid grid-cols-[8rem_1fr] gap-y-2 text-sm">
        <dt className="text-pt-muted">送信元</dt>
        <dd>{mail.sender}</dd>
        <dt className="text-pt-muted">許可</dt>
        <dd data-testid="mail-state">
          {STATE_LABEL[mail.state] ?? mail.state}
          {granted && mail.granted_at !== null && <span className="ml-2 text-pt-muted">（{formatShortDateTime(mail.granted_at)} に許可）</span>}
        </dd>
      </dl>
      {mail.paused && (
        <div className="mt-4">
          <Notice tone="warn" title="Gmail に送信を断られました">
            送信の上限などの理由で、10 分ほど確認コードを送りません。続くときは送信元の人の Gmail を確かめてください。
          </Notice>
        </div>
      )}
      {mail.state === 'none' && (
        <div className="mt-4">
          <Notice tone="warn" title="まだ許可されていません">
            許可されるまで、確認コードを送れません。顧客は新しい端末からログインできません（ログイン中の人と、覚えた端末からのログインは使えます）。
          </Notice>
        </div>
      )}
      {mail.state === 'expired' && (
        <div className="mt-4">
          <Notice tone="danger" title="許可が切れています">
            確認コードを送れません。送信元の人がこの画面を開き、もう一度許可してください。
          </Notice>
        </div>
      )}
      <div className="mt-5 flex flex-wrap items-center gap-3">
        {mail.is_sender ? (
          <Button variant="primary" onClick={() => { window.location.assign(`${API}/mail/authorize`); }}>
            {granted ? '許可し直す' : '送信元の Gmail を許可'}
          </Button>
        ) : (
          <p className="text-sm text-pt-muted">許可は送信元の人だけが押せます。送信元の人にこの画面を開いてもらってください。</p>
        )}
        {(granted || mail.state === 'expired') && (
          <Button disabled={busy} onClick={() => { void revoke(); }}>
            許可を取り消す
          </Button>
        )}
      </div>
      <ErrorLine message={error} />
      <p className="mt-5 text-xs text-pt-muted">
        確認コードのメールは送信元の人の Gmail から送ります。送ったメールは送信元の人の送信済みフォルダに残り、顧客が返信すると送信元の人の受信箱に届きます。{'許可はメールを送る範囲だけで、受信箱は読みません。'}
      </p>
    </Card>
  );
}

export function MailView({ status }: { status: PortalAdminStatus | null }) {
  const [load, reload] = useGet<PortalAdminMail>(`${API}/mail`);
  return (
    <Page current="mail" title="メールの送信元" status={status}>
      {load.state === 'loading' && <Loading />}
      {load.state === 'error' && <LoadError message={load.message} />}
      {load.state === 'ok' && <MailBody mail={load.data} onChanged={reload} />}
    </Page>
  );
}
