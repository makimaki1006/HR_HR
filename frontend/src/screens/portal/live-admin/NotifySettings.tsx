// 応募の通知 (計画 14 章): シートごとの送り先のタブと、通知の状態。
import { useState, type ReactNode } from 'react';
import type { PortalAdminNotify } from '../../../generated/PortalAdminNotify';
import type { PortalAdminSheet } from '../../../generated/PortalAdminSheet';
import { Badge, Button, Notice, formatShortDateTime } from '../ui/kit';
import { API, put } from './api';
import { notifySummary } from './model';
import { ErrorLine, TextField } from './parts';

export function NotifyBadge({ notify }: { notify: PortalAdminNotify }) {
  const n = notifySummary(notify);
  return <Badge tone={n.tone}>{n.text}</Badge>;
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-wrap gap-x-4 gap-y-1 border-t border-pt-line py-2 first:border-t-0">
      <dt className="w-44 shrink-0 text-pt-muted">{label}</dt>
      <dd className="text-pt-ink">{children}</dd>
    </div>
  );
}

export function NotifySettings({ sheet, onSaved }: { sheet: PortalAdminSheet; onSaved: () => void }) {
  const n = sheet.notify;
  const [tab, setTab] = useState(n.tab);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const save = async (): Promise<void> => {
    setBusy(true);
    setError(null);
    const r = await put<PortalAdminSheet>(`${API}/sheets/${encodeURIComponent(sheet.id)}`, { notify_tab: tab.trim() });
    setBusy(false);
    if (!r.ok) {
      setError(r.message);
      return;
    }
    onSaved();
  };
  const on = n.tab.trim() !== '';
  return (
    <div className="rounded-lg border border-pt-line bg-pt-surface p-4">
      <p className="mb-3 text-sm text-pt-ink">
        新しい応募者の行が入ったら、送り先のタブに書かれたメールアドレスの全員に、1 人 1 通ずつ知らせます。
        本文には応募者の情報を入れず、画面へのリンクだけを送ります。
      </p>
      <div className="grid gap-3 md:grid-cols-[minmax(0,24rem)_auto]">
        <TextField
          label="送り先のタブの名前"
          hint="このシートの中のタブです。メールアドレスが書いてあれば、列の並びは問いません。保存したタブは、あとで名前を変えても同じタブから読みます。空にすると通知しません。"
          value={tab}
          maxLength={100}
          placeholder="例：通知の送り先"
          onChange={(e) => { setTab(e.target.value); }}
        />
        <div className="flex items-start md:pt-6">
          <Button variant="primary" disabled={busy || tab.trim() === n.tab} onClick={() => { void save(); }}>保存</Button>
        </div>
      </div>
      <ErrorLine message={error} />

      {on && !n.running && (
        <div className="mt-3">
          <Notice tone="info" title="通知はまだ動いていません">
            社内用シート・メールの送信元・顧客のシートを読む設定がそろうと動きます。
          </Notice>
        </div>
      )}
      {on && n.running && n.problem !== null && (
        <div className="mt-3">
          <Notice tone="warn" title="送っていません">{n.problem}</Notice>
        </div>
      )}

      {on && (
        <dl className="mt-3 text-sm">
          <Row label="状態"><NotifyBadge notify={n} /></Row>
          <Row label="読めた送り先">
            {n.recipients === null ? 'まだ読んでいません' : `${String(n.recipients)} 件`}
            {n.skipped > 0 && <span className="ml-2 text-pt-warn">メールアドレスの形でないもの {String(n.skipped)} 件は飛ばしました</span>}
          </Row>
          {n.waiting > 0 && <Row label="まだ知らせていない応募">{`${String(n.waiting)} 件（次の確認で送ります）`}</Row>}
          <Row label="最後に確かめた時刻">{n.last_checked_at === null ? 'まだ確かめていません' : formatShortDateTime(n.last_checked_at)}</Row>
          <Row label="最後に送った時刻">{n.last_sent_at === null ? 'まだ送っていません' : formatShortDateTime(n.last_sent_at)}</Row>
        </dl>
      )}
      {on && (
        <p className="mt-2 text-xs text-pt-muted">
          5 分おきにシートを確かめます。登録した直後と、アプリを入れ直した直後は、今ある行を基準にするだけで送りません。同じ応募者には 1 回だけ送ります。
          新しい応募を見分けるため、「項目の設定」で電話番号かメールアドレスの種類の項目が要ります。
          送る数は全体で 1 時間 100 通・1 日 500 通、このシートで 1 時間 50 通までで、超えた分は保留して後で送ります。
        </p>
      )}
    </div>
  );
}
