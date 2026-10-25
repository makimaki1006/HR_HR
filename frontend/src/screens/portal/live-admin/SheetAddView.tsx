// シートの登録: URL を貼る → タブを選ぶ → 見出しの行を決めて試しに読む → 登録する。
import { useId, useState } from 'react';
import type { PortalAdminCustomerDetail } from '../../../generated/PortalAdminCustomerDetail';
import type { PortalAdminPreview } from '../../../generated/PortalAdminPreview';
import type { PortalAdminProbe } from '../../../generated/PortalAdminProbe';
import type { PortalAdminSheet } from '../../../generated/PortalAdminSheet';
import type { PortalAdminStatus } from '../../../generated/PortalAdminStatus';
import { Button, Field, Notice, TextInput } from '../ui/kit';
import { API, post } from './api';
import { SHEET_URL_PATTERN } from './model';
import { ErrorLine, Page, PreviewCaption, PreviewTable, SelectField, TextField, useGet, useNav } from './parts';

function Step({ n, title, children }: { n: number; title: string; children: React.ReactNode }) {
  return (
    <section className="mb-4 rounded-lg border border-pt-line bg-pt-surface">
      <h2 className="flex items-center gap-2 border-b border-pt-line px-4 py-2.5 text-[13px] font-semibold">
        <span className="pt-num inline-flex h-5 w-5 items-center justify-center rounded-full bg-pt-sunken text-[12px] text-pt-muted">{String(n)}</span>
        {title}
      </h2>
      <div className="p-4">{children}</div>
    </section>
  );
}

/** 共有されていないときに、追加するアドレスを出す */
function NotShared({ email }: { email: string }) {
  return (
    <Notice tone="warn" title="シートを開けません">
      シートの「共有」で、次のアドレスを<strong>編集者</strong>として追加してから、もう一度読んでください。
      <code className="mt-2 block rounded bg-pt-surface px-2 py-1 text-[12px] break-all select-all">{email}</code>
    </Notice>
  );
}

export function SheetAddView({ customerId, status }: { customerId: string; status: PortalAdminStatus | null }) {
  const go = useNav();
  const [customer] = useGet<PortalAdminCustomerDetail>(`${API}/customers/${encodeURIComponent(customerId)}`);
  const [url, setUrl] = useState('');
  const [probe, setProbe] = useState<PortalAdminProbe | null>(null);
  const [tab, setTab] = useState<number | null>(null);
  const [headerRow, setHeaderRow] = useState(1);
  const [exampleRows, setExampleRows] = useState(0);
  const [label, setLabel] = useState('');
  const [preview, setPreview] = useState<PortalAdminPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [shareTo, setShareTo] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const name = customer.state === 'ok' ? customer.data.customer.name : '';
  const urlId = useId();

  const fail = (r: { message: string; error: string; extra: Record<string, unknown> }): void => {
    if (r.error === 'sheet_not_shared' && typeof r.extra.client_email === 'string') {
      setShareTo(r.extra.client_email);
      setError(null);
    } else {
      setError(r.message);
    }
  };

  const readTabs = async (): Promise<void> => {
    setError(null);
    setShareTo(null);
    setProbe(null);
    setPreview(null);
    if (!SHEET_URL_PATTERN.test(url.trim())) {
      setError('Google スプレッドシートの URL を貼ってください（https://docs.google.com/spreadsheets/d/… の形）。');
      return;
    }
    setBusy(true);
    const r = await post<PortalAdminProbe>(`${API}/sheet-probe`, { url });
    setBusy(false);
    if (!r.ok) {
      fail(r);
      return;
    }
    setProbe(r.data);
    const first = r.data.tabs.find((t) => t.sheet_id === r.data.gid) ?? r.data.tabs[0];
    setTab(first?.sheet_id ?? null);
    if (label === '' && first) setLabel(first.title);
  };

  const readPreview = async (): Promise<void> => {
    if (tab === null) return;
    setError(null);
    setShareTo(null);
    setBusy(true);
    const r = await post<PortalAdminPreview>(`${API}/sheet-preview`, { url, sheet_id: tab, header_row: headerRow, example_rows: exampleRows });
    setBusy(false);
    if (!r.ok) {
      fail(r);
      return;
    }
    setPreview(r.data);
  };

  const register = async (): Promise<void> => {
    if (tab === null) return;
    setError(null);
    setBusy(true);
    const r = await post<PortalAdminSheet>(`${API}/customers/${encodeURIComponent(customerId)}/sheets`, {
      url,
      sheet_id: tab,
      label,
      header_row: headerRow,
      example_rows: exampleRows,
    });
    setBusy(false);
    if (!r.ok) {
      fail(r);
      return;
    }
    go({ view: 'sheet', id: r.data.id });
  };

  const previewMatches = preview !== null && preview.header_row === headerRow && preview.example_rows === exampleRows;

  return (
    <Page current="customers" title="シートを登録" status={status} back={{ label: name === '' ? '顧客' : name, to: { view: 'customer', id: customerId } }}>
      <Step n={1} title="シートの URL">
        <Field label="Google スプレッドシートの URL" htmlFor={urlId} hint="ブラウザでシートを開いたときのアドレスを貼ります。見せたいタブを開いた状態の URL なら、そのタブを選んでおきます。">
          <div className="flex gap-2">
            <TextInput id={urlId} value={url} placeholder="https://docs.google.com/spreadsheets/d/…" onChange={(e) => { setUrl(e.target.value); setProbe(null); setPreview(null); }} onKeyDown={(e) => { if (e.key === 'Enter') void readTabs(); }} />
            <Button variant="primary" disabled={busy || url.trim() === ''} onClick={() => { void readTabs(); }}>読む</Button>
          </div>
        </Field>
      </Step>

      {probe !== null && (
        <Step n={2} title="タブと見出しの行">
          <div className="grid gap-3 md:grid-cols-4">
            <SelectField label="タブ" value={tab === null ? '' : String(tab)} onChange={(e) => { setTab(Number(e.target.value)); setPreview(null); }}>
              {probe.tabs.map((t) => <option key={t.sheet_id} value={String(t.sheet_id)}>{t.title}</option>)}
            </SelectField>
            <TextField label="見出しの行" hint="項目の名前が並んでいる行（1〜20）。" type="number" min={1} max={20} value={String(headerRow)} onChange={(e) => { setHeaderRow(Math.max(1, Math.min(20, Number(e.target.value) || 1))); }} />
            <TextField label="記入例の行数" hint="見出しのすぐ下にある記入例の行（応募者ではない）。" type="number" min={0} max={10} value={String(exampleRows)} onChange={(e) => { setExampleRows(Math.max(0, Math.min(10, Number(e.target.value) || 0))); }} />
            <TextField label="顧客に見せる名前" hint="顧客の画面でのシートの名前（50 文字まで）。" value={label} maxLength={50} onChange={(e) => { setLabel(e.target.value); }} />
          </div>
          <div className="mt-3">
            <Button disabled={busy || tab === null} onClick={() => { void readPreview(); }}>試しに読む</Button>
          </div>
          {preview !== null && (
            <div className="mt-4">
              <PreviewCaption preview={preview} />
              <PreviewTable preview={preview} />
            </div>
          )}
        </Step>
      )}

      {shareTo !== null && <div className="mb-4"><NotShared email={shareTo} /></div>}
      <ErrorLine message={error} />

      {probe !== null && (
        <div className="flex flex-wrap items-center gap-3">
          <Button variant="primary" disabled={busy || !previewMatches || label.trim() === ''} onClick={() => { void register(); }}>このシートを登録する</Button>
          {!previewMatches && <span className="text-xs text-pt-muted">登録の前に「試しに読む」で中身を確かめます。</span>}
          {previewMatches && <span className="text-xs text-pt-muted">登録したあと、顧客に見せる項目・書ける項目を設定します（最初は何も見せません）。</span>}
        </div>
      )}
    </Page>
  );
}
