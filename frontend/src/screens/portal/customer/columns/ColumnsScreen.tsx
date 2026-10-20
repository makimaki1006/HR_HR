import { useEffect, useRef, useState, type SyntheticEvent } from 'react';
import { actions, useData, currentSheet } from '../../data';
import { KIND_LABEL, KIND_ORDER, kindOf, labelOf } from '../../columns';
import type { ColumnConfig, ColumnKind, SheetConfig } from '../../types';
import { IconEye, IconPlus } from '../../ui/icons';
import { Button, Dialog, Field, IconButton, Select, TextInput, cx, formatShortDateTime } from '../../ui/kit';
import { AppShell } from '../../ui/shell';
import { Detail, type DetailHandle } from './Detail';
import { IconArrowDown, IconArrowUp, IconEyeOff } from './icons';
import { KIND_ICON, Toggle, useEscape } from './parts';

function AddColumn({ sheet, onClose, onAdded }: { sheet: SheetConfig; onClose: () => void; onAdded: (h: string) => void }) {
  const [name, setName] = useState('');
  const [kind, setKind] = useState<ColumnKind>('text');
  useEscape(true, onClose);
  const trimmed = name.trim();
  const dup = sheet.columns.some((c) => c.header === trimmed);
  const add = (e?: SyntheticEvent) => {
    e?.preventDefault();
    if (trimmed === '' || dup) return;
    const col: ColumnConfig = { header: trimmed, label: '', visible: true, writable: true, options: kind === 'choice' ? ['選択肢1'] : null, multiline: false, kind, group: 'customer', inList: false };
    actions.updateColumns(sheet.id, [...sheet.columns, col], `項目「${trimmed}」を追加`);
    onAdded(trimmed);
    onClose();
  };
  return (
    <Dialog open title="項目を足す" width="sm" onClose={onClose} footer={<><Button onClick={onClose}>やめる</Button><Button variant="primary" disabled={trimmed === '' || dup} onClick={() => { add(); }}>足す</Button></>}>
      <form className="flex flex-col gap-3" onSubmit={add}>
        <Field label="項目の名前" error={dup ? '同じ名前の項目があります。' : undefined}>
          <TextInput value={name} autoFocus onChange={(e) => { setName(e.target.value); }} placeholder="例: 内定承諾日" />
        </Field>
        <Field label="種類">
          <Select value={kind} onChange={(e) => { setKind(e.target.value as ColumnKind); }}>
            {KIND_ORDER.map((k) => <option key={k} value={k}>{KIND_LABEL[k]}</option>)}
          </Select>
        </Field>
        <p className="text-xs text-pt-muted">シートにも列が足されます。</p>
        <button type="submit" className="hidden" aria-hidden="true" tabIndex={-1} />
      </form>
    </Dialog>
  );
}

function ChangeLog({ sheetId }: { sheetId: string }) {
  const d = useData();
  const list = d.activity.filter((e) => e.kind === 'column_change' && e.sheetId === sheetId).slice(0, 5);
  return (
    <details className="group mt-4 max-w-[560px] rounded-lg border border-pt-line bg-pt-surface">
      <summary className="flex h-10 cursor-pointer list-none items-center gap-2 px-4 text-[13px] font-medium">
        <span className="text-pt-faint transition-transform group-open:rotate-90" aria-hidden="true">›</span>
        変更の記録
        <span className="pt-num text-xs font-normal text-pt-faint">最新 {list.length} 件</span>
      </summary>
      <ul className="divide-y divide-pt-line border-t border-pt-line">
        {list.length === 0 && <li className="px-4 py-3 text-[13px] text-pt-muted">まだ変更はありません。</li>}
        {list.map((e) => (
          <li key={e.id} className="flex flex-wrap items-baseline gap-x-3 px-4 py-2 text-[13px]">
            <span className="pt-num w-24 shrink-0 text-xs text-pt-muted">{formatShortDateTime(e.at)}</span>
            <span className="min-w-0 flex-1">{e.detail}</span>
            <span className="text-xs text-pt-muted">{e.actorName}</span>
          </li>
        ))}
      </ul>
    </details>
  );
}

function orderDetail(before: ColumnConfig[], after: ColumnConfig[]): string {
  const moved = after.filter((c, i) => before[i]?.header !== c.header);
  if (moved.length === 2 && moved[0] && moved[1]) return `「${labelOf(moved[0])}」と「${labelOf(moved[1])}」の順番を入れ替え`;
  const first = moved[0];
  return first ? `項目の並びを変更（「${labelOf(first)}」ほか ${String(moved.length)} 項目）` : '項目の並びを変更';
}

export function ColumnsScreen() {
  const d = useData();
  const sheet = currentSheet(d);
  const isStaff = d.viewer.kind === 'staff';
  // 表の見出しの「この項目の設定」から来たときは、その項目を選んだ状態で開く（一度読んだら消す）。
  const [initialPick] = useState<{ sheetId: string; header: string } | null>(() => {
    try {
      const raw = sessionStorage.getItem('pt.columns.pick');
      sessionStorage.removeItem('pt.columns.pick');
      return raw ? (JSON.parse(raw) as { sheetId: string; header: string }) : null;
    } catch {
      return null;
    }
  });
  const [picked, setPicked] = useState<{ sheetId: string; header: string } | null>(initialPick);
  const [step, setStep] = useState<'list' | 'detail'>(initialPick ? 'detail' : 'list');
  const [addingFor, setAddingFor] = useState<string | null>(null);
  const [sortMode, setSortMode] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [guard, setGuard] = useState<string | null>(null);
  const [orderDraft, setOrderDraft] = useState<{ sheetId: string; cols: ColumnConfig[]; from: ColumnConfig[] } | null>(null);
  const detailRef = useRef<DetailHandle | null>(null);
  const timer = useRef<number | null>(null);
  const orderRef = useRef<typeof orderDraft>(null);

  const flush = () => {
    if (timer.current !== null) { window.clearTimeout(timer.current); timer.current = null; }
    const o = orderRef.current;
    if (!o) return;
    orderRef.current = null;
    setOrderDraft(null);
    actions.updateColumns(o.sheetId, o.cols, orderDetail(o.from, o.cols));
  };
  const flushRef = useRef(flush);
  useEffect(() => { flushRef.current = flush; });
  useEffect(() => () => { flushRef.current(); }, []);

  const cancelGuard = () => { setGuard(null); };
  useEscape(guard !== null, cancelGuard);

  if (!sheet || sheet.columns.length === 0) {
    return <AppShell section="columns" title="項目の設定"><p className="text-[13px] text-pt-muted">このシートには、まだ設定できる項目がありません。</p></AppShell>;
  }
  const canEdit = isStaff || sheet.customerCanEditColumns === true;
  const adding = addingFor === sheet.id && canEdit;
  const cols = orderDraft?.sheetId === sheet.id ? orderDraft.cols : sheet.columns;
  const selected = (picked?.sheetId === sheet.id ? cols.find((c) => c.header === picked.header) : undefined) ?? cols[0];
  const rows = d.rows[sheet.id] ?? [];
  const selectedLatest = sheet.columns.find((c) => c.header === selected?.header) ?? selected;

  const move = (i: number, dir: -1 | 1) => {
    const j = i + dir;
    const a = cols[i];
    const b = cols[j];
    if (!a || !b) return;
    const next = [...cols];
    next[i] = b;
    next[j] = a;
    const nd = { sheetId: sheet.id, cols: next, from: orderDraft?.from ?? sheet.columns };
    orderRef.current = nd;
    setOrderDraft(nd);
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => { flushRef.current(); }, 2500);
  };
  const toggle = (c: ColumnConfig) => {
    flush();
    const latest = sheet.columns.find((x) => x.header === c.header) ?? c;
    actions.updateColumns(sheet.id, sheet.columns.map((x) => (x.header === c.header ? { ...x, visible: !latest.visible } : x)), `「${labelOf(c)}」を${latest.visible ? '画面に出さない' : '画面に出す'}設定に変更`);
  };
  const go = (header: string, toDetail = true) => {
    const sameOne = header === selected?.header;
    if (dirty && !sameOne) { setGuard(header); return; }
    setPicked({ sheetId: sheet.id, header });
    if (toDetail) setStep('detail');
  };
  const back = () => {
    if (dirty) { setGuard('\u0000list'); return; }
    setStep('list');
  };
  const proceed = (target: string) => {
    setGuard(null);
    if (target === '\u0000list') { setStep('list'); return; }
    setPicked({ sheetId: sheet.id, header: target });
    setStep('detail');
  };

  return (
    <AppShell section="columns" title="項目の設定">
      {!canEdit && <p className="mb-3 text-[13px] text-pt-muted">見るだけの画面です。変更は弊社にご依頼ください。</p>}
      <div className="grid gap-4 lg:grid-cols-[320px_minmax(0,1fr)] lg:items-start">
        <section className={cx('rounded-lg border border-pt-line bg-pt-surface', step === 'detail' && 'hidden lg:block')}>
          <div className="flex items-center gap-2 border-b border-pt-line px-3 py-2">
            <h2 className="text-[13px] font-semibold">項目</h2>
            <span className="pt-num text-xs text-pt-faint">{cols.length}</span>
            <div className="ml-auto flex items-center gap-1">
              {canEdit && <Button size="sm" className="sm:hidden" aria-pressed={sortMode} onClick={() => { setSortMode(!sortMode); }}>{sortMode ? '完了' : '並べ替え'}</Button>}
              {canEdit && <Button size="sm" onClick={() => { setAddingFor(sheet.id); }}><IconPlus size={14} />項目を足す</Button>}
            </div>
          </div>
          <ul>
            {cols.map((c, i) => {
              const Icon = KIND_ICON[kindOf(c)];
              const active = c.header === selected?.header;
              const showAll = active || sortMode;
              return (
                <li key={c.header} className={cx('group flex items-center border-b border-pt-line last:border-b-0', active && 'bg-pt-primary-soft/60')}>
                  <button
                    type="button"
                    onClick={() => { go(c.header); }}
                    aria-current={active ? 'true' : undefined}
                    className={cx('flex h-11 min-w-0 flex-1 items-center gap-2 pl-3 text-left text-[13px] sm:h-9', !c.visible && 'text-pt-faint')}
                  >
                    <span className="text-pt-muted"><Icon size={14} /></span>
                    <span className={cx('truncate', active && 'font-medium')}>{labelOf(c)}</span>
                    {active && dirty && <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-pt-warn" aria-label="保存していない変更があります" />}
                    <span className="ml-auto flex shrink-0 gap-1 pr-1 text-[11px] text-pt-faint">
                      {c.writable && <span className="rounded bg-pt-sunken px-1">記入</span>}
                      {c.inList && <span className="rounded bg-pt-sunken px-1">カード</span>}
                    </span>
                  </button>
                  {canEdit && (
                    <div className="flex shrink-0 pr-1">
                      <span className={cx(showAll || !c.visible ? 'flex' : 'hidden sm:flex sm:opacity-0 sm:group-hover:opacity-100 sm:focus-within:opacity-100')}>
                        <IconButton label={c.visible ? '画面に出さない' : '画面に出す'} onClick={() => { toggle(c); }}>{c.visible ? <IconEye size={14} /> : <IconEyeOff size={14} />}</IconButton>
                      </span>
                      <span className={cx(showAll ? 'flex' : 'hidden sm:flex sm:opacity-0 sm:group-hover:opacity-100 sm:focus-within:opacity-100')}>
                        <IconButton label="上へ" disabled={i === 0} onClick={() => { move(i, -1); }}><IconArrowUp size={14} /></IconButton>
                        <IconButton label="下へ" disabled={i === cols.length - 1} onClick={() => { move(i, 1); }}><IconArrowDown size={14} /></IconButton>
                      </span>
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
          {isStaff && (
            <div className="flex items-center gap-2 border-t border-pt-line px-3 py-2.5">
              <span className="text-[13px]">顧客にも項目の変更を許す</span>
              <span className="ml-auto">
                <Toggle label="顧客にも項目の変更を許す" checked={sheet.customerCanEditColumns === true} onChange={(v) => { actions.updateSheet(sheet.id, { customerCanEditColumns: v }, v ? '顧客にも項目の変更を許可' : '顧客の項目の変更を止める'); }} />
              </span>
            </div>
          )}
        </section>

        <div className={cx('min-w-0', step === 'list' && 'hidden lg:block')}>
          {selectedLatest && (
            <Detail
              key={sheet.id + '/' + selectedLatest.header}
              ref={detailRef}
              sheet={sheet}
              col={selectedLatest}
              rows={rows}
              canEdit={canEdit}
              isStaff={isStaff}
              onBack={back}
              onDirty={setDirty}
              onToggleVisible={toggle}
            />
          )}
          <ChangeLog sheetId={sheet.id} />
        </div>
      </div>
      {adding && <AddColumn sheet={sheet} onClose={() => { setAddingFor(null); }} onAdded={(h) => { setPicked({ sheetId: sheet.id, header: h }); setStep('detail'); }} />}
      <Dialog
        open={guard !== null}
        title="保存していない変更があります"
        width="sm"
        onClose={cancelGuard}
        footer={
          <>
            <Button onClick={cancelGuard}>戻る</Button>
            <Button onClick={() => { const t = guard; detailRef.current?.discard(); if (t !== null) proceed(t); }}>変更を捨てる</Button>
            <Button variant="primary" onClick={() => { const t = guard; if (t !== null) { setGuard(null); detailRef.current?.save(() => { proceed(t); }); } }}>保存する</Button>
          </>
        }
      >
        <p className="text-[13px]">「{selected ? labelOf(selected) : ''}」の変更を保存しますか。</p>
      </Dialog>
    </AppShell>
  );
}
