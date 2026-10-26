// 登録したシート: 設定・試しに読んだ結果・項目の設定。
import { useState } from 'react';
import type { PortalAdminColumn } from '../../../generated/PortalAdminColumn';
import type { PortalAdminColumns } from '../../../generated/PortalAdminColumns';
import type { PortalAdminPreview } from '../../../generated/PortalAdminPreview';
import type { PortalAdminSheet } from '../../../generated/PortalAdminSheet';
import type { PortalAdminStatus } from '../../../generated/PortalAdminStatus';
import { Badge, Button, IconButton, Notice, Select, TextInput } from '../ui/kit';
import { IconChevronDown, IconExternal, IconRefresh } from '../ui/icons';
import { API, put } from './api';
import { duplicateHeaders, GROUP_OPTIONS, KIND_OPTIONS, missingHeaders, moveColumn, setColumn, withNewHeaders } from './model';
import { ErrorLine, LoadError, Loading, Page, PreviewCaption, PreviewTable, Section, TD, TH, Table, TextField, useGet } from './parts';

function SheetSettings({ sheet, onSaved }: { sheet: PortalAdminSheet; onSaved: () => void }) {
  const [label, setLabel] = useState(sheet.label);
  const [headerRow, setHeaderRow] = useState(sheet.header_row);
  const [exampleRows, setExampleRows] = useState(sheet.example_rows);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const save = async (body: Record<string, unknown>): Promise<void> => {
    setBusy(true);
    setError(null);
    const r = await put<PortalAdminSheet>(`${API}/sheets/${encodeURIComponent(sheet.id)}`, body);
    setBusy(false);
    if (!r.ok) setError(r.message);
    onSaved();
  };
  const dirty = label.trim() !== sheet.label || headerRow !== sheet.header_row || exampleRows !== sheet.example_rows;
  return (
    <div className="rounded-lg border border-pt-line bg-pt-surface p-4">
      <div className="grid gap-3 md:grid-cols-4">
        <TextField label="顧客に見せる名前" value={label} maxLength={50} onChange={(e) => { setLabel(e.target.value); }} />
        <TextField label="見出しの行" hint="変えて保存すると、その行の見出しで項目を作り直します（同じ見出しの設定は引き継ぎます）。" type="number" min={1} max={20} value={String(headerRow)} onChange={(e) => { setHeaderRow(Math.max(1, Math.min(20, Number(e.target.value) || 1))); }} />
        <TextField label="記入例の行数" type="number" min={0} max={10} value={String(exampleRows)} onChange={(e) => { setExampleRows(Math.max(0, Math.min(10, Number(e.target.value) || 0))); }} />
        <div className="flex items-end">
          <Button variant="primary" disabled={!dirty || busy} onClick={() => { void save({ label, header_row: headerRow, example_rows: exampleRows }); }}>保存</Button>
        </div>
      </div>
      <div className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-2 text-sm">
        <label className="inline-flex items-center gap-2">
          <input type="checkbox" checked={sheet.customer_can_edit_columns} disabled={busy} onChange={(e) => { void save({ customer_can_edit_columns: e.target.checked }); }} />
          顧客も項目の設定を変えてよい
        </label>
        <label className="inline-flex items-center gap-2">
          <input type="checkbox" checked={sheet.disabled} disabled={busy} onChange={(e) => { void save({ disabled: e.target.checked }); }} />
          このシートを止める（顧客の画面に出さない）
        </label>
        <a href={sheet.url} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1 text-pt-primary hover:underline">シートを開く<IconExternal size={12} /></a>
      </div>
      <ErrorLine message={error} />
    </div>
  );
}

function check(label: string, on: boolean, onChange: (v: boolean) => void) {
  return <input type="checkbox" aria-label={label} checked={on} onChange={(e) => { onChange(e.target.checked); }} />;
}

export function ColumnsEditor({ initial, headers, sheetId, onSaved }: { initial: PortalAdminColumn[]; headers: string[] | null; sheetId: string; onSaved: (c: PortalAdminColumns) => void }) {
  const [cols, setCols] = useState<PortalAdminColumn[]>(() => (headers === null ? initial : withNewHeaders(initial, headers)));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const gone = headers === null ? [] : missingHeaders(cols, headers);
  const dup = headers === null ? [] : duplicateHeaders(headers);
  const edit = (i: number, patch: Partial<PortalAdminColumn>): void => { setSaved(false); setCols((c) => setColumn(c, i, patch)); };
  const move = (i: number, d: -1 | 1): void => { setSaved(false); setCols((c) => moveColumn(c, i, d)); };
  const save = async (): Promise<void> => {
    setBusy(true);
    setError(null);
    const r = await put<PortalAdminColumns>(`${API}/sheets/${encodeURIComponent(sheetId)}/columns`, { columns: cols });
    setBusy(false);
    if (!r.ok) {
      setError(r.message);
      return;
    }
    setSaved(true);
    onSaved(r.data);
  };
  return (
    <>
      {dup.length > 0 && (
        <div className="mb-3">
          <Notice tone="warn" title="シートに同じ見出しが 2 つ以上あります">
            {dup.join('、')}。どちらの列の値か決められないので、顧客の画面には出しません。シートの見出しを別の名前にしてください。
          </Notice>
        </div>
      )}
      {gone.length > 0 && (
        <div className="mb-3">
          <Notice tone="warn" title="シートの見出しに無い項目があります">
            {gone.join('、')}。シートの見出しを変えた・列を消した可能性があります。不要なら「見せる」を外します。
          </Notice>
        </div>
      )}
      <Table minWidth={980}>
        <thead>
          <tr>
            <th className={TH + ' w-16'}>並び</th>
            <th className={TH}>シートの見出し</th>
            <th className={TH}>画面での名前</th>
            <th className={TH + ' text-center'}>見せる</th>
            <th className={TH + ' text-center'}>書ける</th>
            <th className={TH + ' text-center'}>一覧に出す</th>
            <th className={TH + ' text-center'}>複数行</th>
            <th className={TH}>種類</th>
            <th className={TH}>まとまり</th>
          </tr>
        </thead>
        <tbody>
          {cols.map((c, i) => (
            <tr key={c.header} className={c.visible ? '' : 'text-pt-muted'}>
              <td className={TD + ' whitespace-nowrap'}>
                <IconButton label="上へ" disabled={i === 0} onClick={() => { move(i, -1); }}><IconChevronDown size={14} className="rotate-180" /></IconButton>
                <IconButton label="下へ" disabled={i === cols.length - 1} onClick={() => { move(i, 1); }}><IconChevronDown size={14} /></IconButton>
              </td>
              <td className={TD + ' font-medium'}>{c.header}{gone.includes(c.header) && <span className="ml-2"><Badge tone="warn">シートに無い</Badge></span>}</td>
              <td className={TD}><TextInput value={c.label} placeholder={c.header} maxLength={50} aria-label={`${c.header}の画面での名前`} className="sm:!h-7" onChange={(e) => { edit(i, { label: e.target.value }); }} /></td>
              <td className={TD + ' text-center'}>{check(`${c.header}を見せる`, c.visible, (v) => { edit(i, { visible: v }); })}</td>
              <td className={TD + ' text-center'}>{check(`${c.header}を書ける`, c.writable, (v) => { edit(i, { writable: v }); })}</td>
              <td className={TD + ' text-center'}>{check(`${c.header}を一覧に出す`, c.in_list, (v) => { edit(i, { in_list: v }); })}</td>
              <td className={TD + ' text-center'}>{check(`${c.header}を複数行にする`, c.multiline, (v) => { edit(i, { multiline: v }); })}</td>
              <td className={TD}>
                <Select value={c.kind} aria-label={`${c.header}の種類`} className="sm:!h-7" onChange={(e) => { edit(i, { kind: e.target.value }); }}>
                  {KIND_OPTIONS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
                </Select>
              </td>
              <td className={TD}>
                <Select value={c.group} aria-label={`${c.header}のまとまり`} className="sm:!h-7" onChange={(e) => { edit(i, { group: e.target.value }); }}>
                  {GROUP_OPTIONS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
                </Select>
              </td>
            </tr>
          ))}
          {cols.length === 0 && <tr><td colSpan={9} className="px-3 py-8 text-center text-pt-muted">項目がありません。シートを読み直すと見出しから足します。</td></tr>}
        </tbody>
      </Table>
      <div className="mt-3 flex flex-wrap items-center gap-3">
        <Button variant="primary" disabled={busy} onClick={() => { void save(); }}>項目の設定を保存</Button>
        <span className="text-xs text-pt-muted">見せない項目は顧客の画面に出ず、書けません。書ける項目だけ顧客が画面から書き換えられます。</span>
        {saved && <span className="text-xs text-pt-ok">保存しました</span>}
      </div>
      <ErrorLine message={error} />
    </>
  );
}

export function SheetView({ id, status }: { id: string; status: PortalAdminStatus | null }) {
  const [load, reload, replace] = useGet<PortalAdminColumns>(`${API}/sheets/${encodeURIComponent(id)}/columns`);
  const [previewLoad, reread] = useGet<PortalAdminPreview>(`${API}/sheets/${encodeURIComponent(id)}/preview`, 'POST');

  if (load.state !== 'ok') {
    return (
      <Page current="customers" title="シート" status={status} back={{ label: '顧客の一覧', to: { view: 'customers' } }}>
        {load.state === 'loading' ? <Loading /> : <LoadError message={load.message} />}
      </Page>
    );
  }
  const { sheet, columns } = load.data;
  const headers = previewLoad.state === 'ok' ? previewLoad.data.headers : null;
  return (
    <Page
      current="customers"
      title={sheet.label}
      badge={sheet.disabled ? <Badge tone="neutral">停止中</Badge> : undefined}
      status={status}
      back={{ label: '顧客', to: { view: 'customer', id: sheet.customer_id } }}
    >
      <p className="-mt-2 mb-3 text-xs text-pt-muted">タブ「{sheet.tab_title}」・見出しは {String(sheet.header_row)} 行目</p>
      <SheetSettings key={`${sheet.label}|${String(sheet.header_row)}|${String(sheet.example_rows)}`} sheet={sheet} onSaved={() => { reload(); reread(); }} />

      <Section title="試しに読んだ結果" actions={<Button size="sm" onClick={reread}><IconRefresh size={14} />読み直す</Button>}>
        {previewLoad.state === 'loading' && <Loading />}
        {previewLoad.state === 'error' && <LoadError message={previewLoad.message} />}
        {previewLoad.state === 'ok' && (
          <>
            <PreviewCaption preview={previewLoad.data} />
            <PreviewTable preview={previewLoad.data} />
          </>
        )}
      </Section>

      <Section title="項目の設定">
        {previewLoad.state === 'loading' ? (
          <Loading />
        ) : (
          <ColumnsEditor key={`${columns.map((c) => c.header).join('|')}#${headers?.join('|') ?? ''}`} initial={columns} headers={headers} sheetId={sheet.id} onSaved={replace} />
        )}
      </Section>
    </Page>
  );
}
