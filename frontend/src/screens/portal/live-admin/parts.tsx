// 社内の管理画面の共通部品。
import { createContext, useCallback, useContext, useEffect, useId, useState, type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes } from 'react';
import type { PortalAdminPreview } from '../../../generated/PortalAdminPreview';
import type { PortalAdminStatus } from '../../../generated/PortalAdminStatus';
import { AdminLayout, type AdminSection } from '../ui/layouts';
import { Button, Dialog, Field, Notice, Select, TextInput, formatShortDateTime } from '../ui/kit';
import { get, type Result } from './api';
import { routeHref, type Route } from './model';

// ---- 画面の中の移動 ----

export const NavContext = createContext<(r: Route) => void>(() => undefined);

export function useNav(): (r: Route) => void {
  return useContext(NavContext);
}

/** 画面の中のリンク。href があるので新しいタブでも開ける */
export function Link({ to, children, className }: { to: Route; children: ReactNode; className?: string }) {
  const go = useNav();
  return (
    <a
      href={routeHref(to)}
      className={className ?? 'text-pt-primary hover:underline'}
      onClick={(e) => {
        if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
        e.preventDefault();
        go(to);
      }}
    >
      {children}
    </a>
  );
}

// ---- 読み込み ----

export type Load<T> = { state: 'loading' } | { state: 'ok'; data: T } | { state: 'error'; message: string; status: number };

/** GET して持つ。reload で読み直す */
export function useGet<T>(path: string | null): [Load<T>, () => void, (data: T) => void] {
  const [n, setN] = useState(0);
  const [result, setResult] = useState<{ key: string; load: Load<T> } | null>(null);
  const key = `${path ?? ''}#${String(n)}`;
  useEffect(() => {
    if (path === null) return undefined;
    const c = new AbortController();
    void get<T>(path, c.signal).then((r: Result<T>) => {
      if (c.signal.aborted) return;
      setResult({ key, load: r.ok ? { state: 'ok', data: r.data } : { state: 'error', message: r.message, status: r.status } });
    });
    return () => { c.abort(); };
  }, [path, key]);
  const reload = useCallback(() => { setN((x) => x + 1); }, []);
  const replace = useCallback((data: T) => { setResult({ key, load: { state: 'ok', data } }); }, [key]);
  const load: Load<T> = result !== null && result.key === key ? result.load : { state: 'loading' };
  return [load, reload, replace];
}

export function Loading() {
  return <p className="py-10 text-center text-sm text-pt-muted">読み込み中…</p>;
}

export function LoadError({ message }: { message: string }) {
  return <Notice tone="danger" title="読み込めませんでした">{message}</Notice>;
}

// ---- 外枠 ----

const STATUS_PATH = '/api/admin/portal/status';

/** 保存先の状態の帯。保存待ち・読めないときだけ目立たせる */
export function StatusBanner({ status }: { status: PortalAdminStatus | null }) {
  if (status === null) return null;
  if (status.store_kind === 'none') {
    return (
      <Notice tone="warn" title="社内用シートがまだ設定されていません">
        顧客・シート・アカウントを保存する社内用シートと、その読み書きの鍵が設定されると使えるようになります。
      </Notice>
    );
  }
  if (!status.ready) {
    return (
      <Notice tone="warn" title="社内用シートを読み込んでいます">
        {status.last_error ?? '少し待ってから開き直してください。'}
      </Notice>
    );
  }
  if (status.pending > 0) {
    return (
      <Notice tone="danger" title={`社内用シートへの保存待ちが ${String(status.pending)} 件あります`}>
        {status.last_error ?? '裏で書き込んでいます。'} 変更はこの画面ではもう効いていますが、保存が終わる前にアプリが再起動すると、その変更は失われます。
      </Notice>
    );
  }
  return null;
}

export function StatusLine({ status }: { status: PortalAdminStatus | null }) {
  if (status === null || status.store_kind === 'none') return null;
  const where = status.store_kind === 'memory' ? '手元の確認用（保存しない）' : '社内用シート';
  return (
    <p className="mt-6 text-xs text-pt-faint" data-testid="store-line">
      保存先: {where}
      {status.last_saved_at !== null && <> ・最後に保存: {formatShortDateTime(status.last_saved_at)}</>}
      {status.loaded_at !== null && <> ・最後に読み込み: {formatShortDateTime(status.loaded_at)}</>}
      {!status.customer_login && <> ・顧客のログインは準備中（確認コードを送るメールの設定待ち）</>}
    </p>
  );
}

export function useStatus(): [PortalAdminStatus | null, () => void] {
  const [load, reload] = useGet<PortalAdminStatus>(STATUS_PATH);
  return [load.state === 'ok' ? load.data : null, reload];
}

export function Page({ current, title, badge, actions, back, status, children }: {
  current: AdminSection;
  title: ReactNode;
  badge?: ReactNode;
  actions?: ReactNode;
  back?: { label: string; to: Route };
  status: PortalAdminStatus | null;
  children: ReactNode;
}) {
  const go = useNav();
  const section = (s: AdminSection): Route => (s === 'accounts' ? { view: 'accounts' } : s === 'activity' ? { view: 'activity' } : { view: 'customers' });
  return (
    <AdminLayout
      title={title}
      current={current}
      onNavigate={(s) => { go(section(s)); }}
      {...(badge ? { badge } : {})}
      {...(actions ? { actions } : {})}
      {...(back ? { back: { label: back.label, onClick: () => { go(back.to); } } } : {})}
    >
      <div className="mb-4 empty:hidden"><StatusBanner status={status} /></div>
      {children}
      <StatusLine status={status} />
    </AdminLayout>
  );
}

export function Section({ title, actions, children }: { title: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <section className="mt-6">
      <div className="mb-2 flex flex-wrap items-center gap-2">
        <h2 className="text-[14px] font-semibold">{title}</h2>
        {actions && <div className="ml-auto flex flex-wrap items-center gap-2">{actions}</div>}
      </div>
      {children}
    </section>
  );
}

// ---- 試しに読んだ結果 ----

/** 見出し・記入例・先頭の応募者の行。値は文字として出す (React がエスケープする) */
export function PreviewTable({ preview }: { preview: PortalAdminPreview }) {
  const width = Math.max(preview.headers.length, ...preview.rows.map((r) => r.length), ...preview.examples.map((r) => r.length));
  const cols = Array.from({ length: width }, (_, i) => i);
  const cell = 'border-b border-r border-pt-line px-2 py-1.5 align-top whitespace-pre-wrap break-words max-w-[240px]';
  return (
    <div className="overflow-x-auto rounded-lg border border-pt-line bg-pt-surface" data-testid="preview-table">
      <table className="text-left text-[12px]">
        <thead>
          <tr>
            <th className={cell + ' bg-pt-sunken text-pt-faint'}>行</th>
            {cols.map((i) => (
              <th key={i} className={cell + ' bg-pt-sunken font-medium text-pt-ink'}>
                {preview.headers[i] !== undefined && preview.headers[i] !== '' ? preview.headers[i] : <span className="text-pt-faint">（見出しなし）</span>}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {preview.examples.map((r, k) => (
            <tr key={`e${String(k)}`} className="bg-pt-bg text-pt-faint">
              <td className={cell}>記入例</td>
              {cols.map((i) => <td key={i} className={cell}>{r[i] ?? ''}</td>)}
            </tr>
          ))}
          {preview.rows.map((r, k) => (
            <tr key={`r${String(k)}`}>
              <td className={cell + ' text-pt-faint'}>{String(k + 1)}</td>
              {cols.map((i) => <td key={i} className={cell}>{r[i] ?? ''}</td>)}
            </tr>
          ))}
          {preview.rows.length === 0 && (
            <tr><td colSpan={width + 1} className="px-3 py-6 text-center text-pt-muted">見出しの下に応募者の行がありません</td></tr>
          )}
        </tbody>
      </table>
    </div>
  );
}

export function PreviewCaption({ preview }: { preview: PortalAdminPreview }) {
  return (
    <p className="mb-2 text-xs text-pt-muted">
      タブ「{preview.tab_title}」の {String(preview.header_row)} 行目を見出しとして読んだ結果（記入例 {String(preview.example_rows)} 行と、その下の先頭 {String(preview.rows.length)} 行）。{formatShortDateTime(preview.read_at)} 時点。応募者の個人情報が含まれます。
    </p>
  );
}

// ---- 招待のリンク ----

export function InviteLinkDialog({ email, path, onClose }: { email: string; path: string | null; onClose: () => void }) {
  const [copied, setCopied] = useState(false);
  const url = path === null ? '' : `${window.location.origin}${path}`;
  return (
    <Dialog
      open={path !== null}
      title="招待のリンク"
      onClose={() => { setCopied(false); onClose(); }}
      footer={<Button variant="primary" onClick={() => { setCopied(false); onClose(); }}>閉じる</Button>}
    >
      <p className="mb-3 text-sm">
        {email} さんに、このリンクを送ってください。1 週間有効で、1 回だけ使えます。本人がパスワードを決めます。
      </p>
      <div className="flex gap-2">
        <TextInput readOnly value={url} aria-label="招待のリンク" onFocus={(e) => { e.currentTarget.select(); }} />
        <Button
          onClick={() => {
            void navigator.clipboard.writeText(url).then(() => { setCopied(true); }, () => { setCopied(false); });
          }}
        >
          {copied ? 'コピーした' : 'コピー'}
        </Button>
      </div>
      <p className="mt-3 text-xs text-pt-muted">このリンクはこの画面でだけ表示します。閉じると見られません（必要なら出し直します。出し直すと前のリンクは使えなくなります）。</p>
    </Dialog>
  );
}

/** 表の外枠。細い線と薄い見出し行 (試作の管理画面と同じ見た目) */
export function Table({ children, minWidth = 900 }: { children: ReactNode; minWidth?: number }) {
  return (
    <div className="overflow-x-auto rounded-lg border border-pt-line bg-pt-surface">
      <table className="w-full text-left text-[13px] [&_tbody_tr:last-child_td]:border-b-0" style={{ minWidth }}>{children}</table>
    </div>
  );
}
export const TH = 'h-8 border-b border-pt-line bg-pt-sunken px-3 text-xs font-medium text-pt-muted whitespace-nowrap';
export const TD = 'h-10 border-b border-pt-line px-3 align-middle';

/** 見出しと結び付いた入力欄 (読み上げ・押せる範囲のため label と id をつなぐ) */
export function TextField({ label, hint, ...input }: { label: string; hint?: ReactNode } & InputHTMLAttributes<HTMLInputElement>) {
  const id = useId();
  return (
    <Field label={label} htmlFor={id} {...(hint !== undefined ? { hint } : {})}>
      <TextInput id={id} {...input} />
    </Field>
  );
}

export function SelectField({ label, hint, children, ...rest }: { label: string; hint?: ReactNode; children: ReactNode } & SelectHTMLAttributes<HTMLSelectElement>) {
  const id = useId();
  return (
    <Field label={label} htmlFor={id} {...(hint !== undefined ? { hint } : {})}>
      <Select id={id} {...rest}>{children}</Select>
    </Field>
  );
}

export function ErrorLine({ message }: { message: string | null }) {
  if (message === null || message === '') return null;
  return <p role="alert" className="mt-2 text-sm text-pt-danger">{message}</p>;
}
