// 1 人の応募者の詳細と編集。一覧（customer/list）から開く。担当: edit チーム。
import { useEffect, useMemo, useRef, useState } from 'react';
import { usePortalStore } from '../../store';
import type { ApplicantRow, ColumnConfig, SheetConfig } from '../../types';
import { Badge, Button, Notice, Select, TextArea, TextInput, cx, formatTime } from '../../ui/kit';
import { ContactLine } from '../../ui/contact';
import { cardLayout, groupOf, kindOf, labelOf } from '../../columns';

export interface RowEditorProps {
  sheet: SheetConfig;
  row: ApplicantRow;
  onClose: () => void;
}

type Values = Record<string, string>;
type Phase =
  | { kind: 'idle' }
  | { kind: 'saving' }
  | { kind: 'saved' }
  | { kind: 'conflict'; latest: Values; auto: { label: string; from: string; to: string }[] }
  | { kind: 'failed' };

const GROUP_TITLES: { group: 'customer' | 'staff'; title: string }[] = [
  { group: 'customer', title: '貴社で記入' },
  { group: 'staff', title: '弊社からの連絡' },
];

/** 列をまとまりに振り分ける。見出しの列は上の見出しに出すので本文に並べない。そのほかは畳む。 */
function groupColumns(columns: ColumnConfig[]): { main: { title: string; cols: ColumnConfig[] }[]; other: ColumnConfig[] } {
  const visible = columns.filter((c) => c.visible);
  const main = GROUP_TITLES.map((g) => ({ title: g.title, cols: visible.filter((c) => groupOf(c) === g.group) })).filter((g) => g.cols.length > 0);
  return { main, other: visible.filter((c) => groupOf(c) === 'other') };
}

function splitDateTime(v: string): { date: string; time: string } {
  const m = /^(\d{4})\/(\d{1,2})\/(\d{1,2})(?:\s+(\d{1,2}):(\d{2}))?/.exec(v);
  if (!m) return { date: '', time: '' };
  const p = (s: string | undefined) => (s ?? '').padStart(2, '0');
  return { date: `${m[1] ?? ''}-${p(m[2])}-${p(m[3])}`, time: m[4] ? `${p(m[4])}:${m[5] ?? '00'}` : '' };
}

function joinDateTime(date: string, time: string): string {
  if (date === '') return '';
  const d = date.replace(/-/g, '/');
  return time === '' ? d : `${d} ${time}`;
}

function ReadValue({ column, value }: { column: ColumnConfig; value: string }) {
  if (value === '') return <span className="text-pt-faint">未記入</span>;
  if (kindOf(column) === 'phone') {
    return <a href={`tel:${value.replace(/[^0-9+]/g, '')}`} className="text-base font-medium text-pt-primary underline underline-offset-2">{value}</a>;
  }
  if (kindOf(column) === 'email') {
    return <a href={`mailto:${value}`} className="break-all text-pt-primary underline underline-offset-2">{value}</a>;
  }
  return <span className={cx('whitespace-pre-wrap break-words', column.multiline && 'leading-relaxed')}>{value}</span>;
}

function ReadItem({ column, value, updated }: { column: ColumnConfig; value: string; updated?: boolean }) {
  return (
    <div className="grid gap-0.5 sm:grid-cols-[7.5rem_1fr] sm:gap-3">
      <dt className="text-xs text-pt-muted sm:pt-0.5 sm:text-sm">
        {labelOf(column)}
        {updated && <span className="ml-1.5 text-pt-warn">更新</span>}
      </dt>
      <dd className="min-w-0 text-sm text-pt-ink">
        <ReadValue column={column} value={value} />
      </dd>
    </div>
  );
}

function EditItem({ column, value, changed, disabled, onChange }: {
  column: ColumnConfig;
  value: string;
  changed: boolean;
  disabled: boolean;
  onChange: (v: string) => void;
}) {
  const id = `edit-${column.header}`;
  let control: React.ReactNode;
  let hint: string | null = null;
  if (column.options) {
    const opts = column.options.includes(value) ? column.options : [...column.options, value];
    control = (
      <Select id={id} value={value} disabled={disabled} onChange={(e) => { onChange(e.target.value); }}>
        {opts.map((o) => <option key={o} value={o}>{o === '' ? '選択なし' : o}</option>)}
      </Select>
    );
  } else if (kindOf(column) === 'datetime') {
    const { date, time } = splitDateTime(value);
    control = (
      <div className="flex gap-2">
        <TextInput id={id} type="date" aria-label="日付" value={date} disabled={disabled} onChange={(e) => { onChange(joinDateTime(e.target.value, time)); }} />
        <TextInput type="time" aria-label="時刻" className="max-w-[8.5rem]" value={time} disabled={disabled || date === ''} onChange={(e) => { onChange(joinDateTime(date, e.target.value)); }} />
      </div>
    );
    hint = date === '' ? '日付を選ぶと時刻を入れられます' : null;
  } else if (column.multiline) {
    control = <TextArea id={id} value={value} rows={4} disabled={disabled} onChange={(e) => { onChange(e.target.value); }} />;
  } else {
    control = <TextInput id={id} value={value} disabled={disabled} onChange={(e) => { onChange(e.target.value); }} />;
  }
  return (
    <div className="flex flex-col gap-1">
      <label htmlFor={id} className="flex items-center gap-2 text-sm font-medium text-pt-ink">
        {labelOf(column)}
        {changed && <span className="text-xs font-normal text-pt-primary">変更あり</span>}
      </label>
      {control}
      {hint && <p className="text-xs text-pt-muted">{hint}</p>}
    </div>
  );
}

function shown(v: string): string {
  return v === '' ? '（空）' : v;
}

export function RowEditor({ sheet, row, onClose }: RowEditorProps) {
  const store = usePortalStore();
  // eslint-disable-next-line @typescript-eslint/unbound-method
  const { markSeen, saveRow, snapshot } = store;
  const [base, setBase] = useState<Values>({ ...row.values });
  const [draft, setDraft] = useState<Values>({ ...row.values });
  const [phase, setPhase] = useState<Phase>({ kind: 'idle' });
  const [confirmClose, setConfirmClose] = useState(false);
  const [failures, setFailures] = useState(0);
  const conflictRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => { markSeen(sheet.id, row.rowNumber); }, [markSeen, sheet.id, row.rowNumber]);

  const snap = snapshot(sheet.id);
  const live = snap.rows.find((r) => r.rowNumber === row.rowNumber)?.values ?? row.values;
  const readTime = formatTime(snap.readAt);

  const { main: groups, other } = useMemo(() => groupColumns(sheet.columns), [sheet.columns]);
  const writable = useMemo(() => sheet.columns.filter((c) => c.visible && c.writable), [sheet.columns]);
  const changedColumns = writable.filter((c) => (draft[c.header] ?? '') !== (base[c.header] ?? ''));
  const dirty = changedColumns.length > 0;
  const busy = phase.kind === 'saving';
  /** 開いたあとにシートで変わった項目（見せる列だけ）。 */
  const sheetChanged = sheet.columns.filter((c) => c.visible && (live[c.header] ?? '') !== (base[c.header] ?? ''));
  const showFresh = sheetChanged.length > 0 && phase.kind !== 'conflict' && phase.kind !== 'saving';

  useEffect(() => {
    if (phase.kind !== 'saved') return undefined;
    const t = setTimeout(() => { setPhase({ kind: 'idle' }); }, 6000);
    return () => { clearTimeout(t); };
  }, [phase]);

  useEffect(() => {
    if (phase.kind === 'conflict') conflictRef.current?.scrollIntoView({ block: 'start', behavior: 'smooth' });
  }, [phase.kind]);

  const set = (h: string, v: string) => {
    setDraft((d) => ({ ...d, [h]: v }));
    if (phase.kind === 'saved' || phase.kind === 'failed') setPhase({ kind: 'idle' });
  };

  const requestClose = () => {
    if (busy) return;
    if (dirty) setConfirmClose(true);
    else onClose();
  };

  /** シートで変わった列（見せる列だけ）。 */
  const conflictColumns = (latest: Values) =>
    sheet.columns.filter((c) => c.visible && (latest[c.header] ?? '') !== (base[c.header] ?? ''));

  const doSave = async (baseValues: Values, next: Values, retried = false) => {
    setPhase({ kind: 'saving' });
    try {
      const res = await saveRow(sheet.id, row.rowNumber, baseValues, next);
      if (res.kind === 'saved') {
        setBase({ ...res.row.values });
        setDraft({ ...res.row.values });
        setFailures(0);
        setPhase({ kind: 'saved' });
      } else if (res.kind === 'conflict') {
        const latest = res.latest.values;
        const mine = new Set(Object.keys(next));
        // 自分が触っていない項目は、聞かずにシートの今の値へ合わせる。
        const auto = conflictColumns(latest)
          .filter((c) => !mine.has(c.header))
          .map((c) => ({ label: labelOf(c), from: base[c.header] ?? '', to: latest[c.header] ?? '' }));
        const follow = (cur: Values): Values => {
          const out = { ...cur };
          for (const c of sheet.columns) if (!mine.has(c.header)) out[c.header] = latest[c.header] ?? '';
          return out;
        };
        setBase(follow);
        setDraft(follow);
        const both = [...mine].filter((h) => (latest[h] ?? '') !== (baseValues[h] ?? ''));
        if (both.length === 0 && !retried) {
          // 同じ項目は変わっていない。最新に合わせて、そのまま保存し直す。
          const nb: Values = {};
          for (const h of mine) nb[h] = latest[h] ?? '';
          setPhase({ kind: 'conflict', latest: { ...latest }, auto });
          await doSave(nb, next, true);
        } else {
          setPhase({ kind: 'conflict', latest: { ...latest }, auto });
        }
      } else {
        setFailures((n) => n + 1);
        setPhase({ kind: 'failed' });
      }
    } catch {
      setFailures((n) => n + 1);
      setPhase({ kind: 'failed' });
    }
  };

  const save = () => {
    if (!dirty || busy) return;
    const baseValues: Values = {};
    const next: Values = {};
    for (const c of changedColumns) {
      baseValues[c.header] = base[c.header] ?? '';
      next[c.header] = draft[c.header] ?? '';
    }
    void doSave(baseValues, next);
  };

  const takeSheet = (latest: Values) => {
    setBase({ ...latest });
    setDraft({ ...latest });
    setPhase({ kind: 'idle' });
  };

  /** 変更のない項目だけ最新に合わせ、入力中の項目はそのまま残す。 */
  const showLatest = () => {
    const mine = new Set(changedColumns.map((c) => c.header));
    setBase((b) => {
      const nb = { ...b };
      for (const c of sheet.columns) if (!mine.has(c.header)) nb[c.header] = live[c.header] ?? '';
      return nb;
    });
    setDraft((d) => {
      const nd = { ...d };
      for (const c of sheet.columns) if (!mine.has(c.header)) nd[c.header] = live[c.header] ?? '';
      return nd;
    });
  };

  /** 自分の入力を残してそのまま保存する。both のときは、両方が書き換えた複数行の項目をシートの内容の後ろに足す。 */
  const keepMine = (latest: Values, both: boolean) => {
    const baseValues: Values = {};
    const next: Values = {};
    const merged: Values = { ...latest };
    for (const c of changedColumns) {
      const mine = draft[c.header] ?? '';
      const theirs = latest[c.header] ?? '';
      const joinable = both && c.multiline && theirs !== (base[c.header] ?? '') && theirs !== '' && theirs !== mine;
      const value = joinable ? `${theirs}\n${mine}` : mine;
      baseValues[c.header] = theirs;
      next[c.header] = value;
      merged[c.header] = value;
    }
    setBase({ ...latest });
    setDraft(merged);
    void doSave(baseValues, next);
  };

  const v = live;
  const layout = cardLayout(sheet);
  const visible = sheet.columns.filter((c) => c.visible);
  const val = (c: ColumnConfig | undefined) => (c ? (v[c.header] ?? '') : '');
  const name = val(layout.title);
  const phoneCol = visible.find((c) => kindOf(c) === 'phone');
  const phone = val(phoneCol);
  const mailCol = visible.find((c) => kindOf(c) === 'email');
  const mail = val(mailCol);
  const summaryCols = visible.filter((c) => groupOf(c) === 'summary' && c !== layout.title && c !== phoneCol && c !== mailCol);
  const metaLine = summaryCols
    .map((c) => ({ c, x: val(c) }))
    .filter((e) => e.x !== '')
    .map((e) => (/^\d+$/.test(e.x) ? `${labelOf(e.c)} ${e.x}` : e.x))
    .join(' ／ ');
  const changedKeys = new Set(sheetChanged.map((c) => c.header));

  return (
    <div className="fixed inset-0 z-[60] flex justify-end" role="dialog" aria-modal="true" aria-label="応募者の詳細">
      <button type="button" aria-label="閉じる" tabIndex={-1} onClick={requestClose} className="hidden flex-1 cursor-default bg-black/30 sm:block" />
      <div className="flex h-full w-full flex-col bg-pt-surface sm:w-[520px] sm:border-l sm:border-pt-line sm:shadow-xl">
        <header className="flex items-start gap-3 border-b border-pt-line px-4 py-3">
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="text-lg font-semibold text-pt-ink">{name}</h2>
              {row.changedSinceLastView === 'new' && <Badge tone="new">新着</Badge>}
              {row.changedSinceLastView === 'updated' && <Badge tone="warn">更新あり</Badge>}
            </div>
            {metaLine !== '' && <p className="mt-1 text-sm text-pt-muted">{metaLine}</p>}
            {mail !== '' && (
              <p className="mt-0.5 text-xs text-pt-muted">
                <a href={`mailto:${mail}`} className="break-all underline underline-offset-2">{mail}</a>
              </p>
            )}
            {phone !== '' && (
              <a href={`tel:${phone.replace(/[^0-9+]/g, '')}`} className="mt-2 inline-flex h-11 items-center rounded-md bg-pt-primary-soft px-4 text-sm font-medium text-pt-primary">
                電話する {phone}
              </a>
            )}
            <p className="mt-2 text-xs text-pt-faint">{readTime} 時点の内容</p>
          </div>
          <Button variant="ghost" className="h-11 min-w-11" disabled={busy} onClick={requestClose}>閉じる</Button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
          {phase.kind === 'conflict' && (() => {
            const mineHeaders = new Set(changedColumns.map((m) => m.header));
            const both = conflictColumns(phase.latest).filter((c) => mineHeaders.has(c.header));
            const joinable = both.some((c) => c.multiline && (phase.latest[c.header] ?? '') !== '');
            return (
              <div ref={conflictRef} className="mb-5 flex flex-col gap-3">
                {phase.auto.length > 0 && (
                  <Notice tone="info" title="シートで変わった項目">
                    <ul>
                      {phase.auto.map((a) => <li key={a.label}>{a.label}: {shown(a.from)} → {shown(a.to)}</li>)}
                    </ul>
                    <p className="mt-1 text-xs text-pt-muted">変えていない項目なので、最新の内容に合わせました。</p>
                  </Notice>
                )}
                {both.length === 0 && (
                  <Button variant="primary" onClick={() => { keepMine(phase.latest, false); }}>もう一度保存</Button>
                )}
                {both.length > 0 && (
                  <Notice tone="warn" title="同じ項目がシートでも変わっています">
                    <p>開いたあとに、シートでも同じ項目が書き換えられました。見比べて選んでください。</p>
                    <ul className="mt-3 flex flex-col gap-3">
                      {both.map((c) => (
                        <li key={c.header} className="rounded-md border border-pt-line bg-pt-surface p-3">
                          <p className="text-sm font-medium">{labelOf(c)}</p>
                          <div className="mt-1 grid gap-2 sm:grid-cols-2">
                            <div>
                              <p className="text-xs text-pt-muted">シートの今の値</p>
                              <p className="whitespace-pre-wrap break-words text-sm">{shown(phase.latest[c.header] ?? '')}</p>
                            </div>
                            <div>
                              <p className="text-xs text-pt-muted">あなたの入力</p>
                              <p className="whitespace-pre-wrap break-words text-sm">{shown(draft[c.header] ?? '')}</p>
                            </div>
                          </div>
                        </li>
                      ))}
                    </ul>
                    <div className="mt-3 flex flex-col gap-2 sm:flex-row sm:flex-wrap">
                      <Button onClick={() => { takeSheet(phase.latest); }}>シートの内容に合わせる</Button>
                      {joinable && <Button onClick={() => { keepMine(phase.latest, true); }}>両方残して保存</Button>}
                      <Button variant="primary" onClick={() => { keepMine(phase.latest, false); }}>自分の入力を残してもう一度保存</Button>
                    </div>
                  </Notice>
                )}
              </div>
            );
          })()}

          <div className="flex flex-col gap-6">
            {groups.map((g) => {
              const readCols = g.cols.filter((c) => !c.writable);
              const editCols = g.cols.filter((c) => c.writable);
              return (
                <section key={g.title}>
                  <h3 className="mb-2 border-b border-pt-line pb-1 text-sm font-semibold text-pt-ink">{g.title}</h3>
                  {readCols.length > 0 && (
                    <dl className="flex flex-col gap-2.5">
                      {readCols.map((c) => <ReadItem key={c.header} column={c} value={v[c.header] ?? ''} updated={changedKeys.has(c.header)} />)}
                    </dl>
                  )}
                  {editCols.length > 0 && (
                    <div className={cx('flex flex-col gap-4', readCols.length > 0 && 'mt-4')}>
                      {editCols.map((c) => (
                        <EditItem
                          key={c.header}
                          column={c}
                          value={draft[c.header] ?? ''}
                          changed={(draft[c.header] ?? '') !== (base[c.header] ?? '')}
                          disabled={busy || phase.kind === 'conflict'}
                          onChange={(val) => { set(c.header, val); }}
                        />
                      ))}
                    </div>
                  )}
                </section>
              );
            })}
            {other.length > 0 && (
              <details className="rounded-md border border-pt-line">
                <summary className="flex h-11 cursor-pointer items-center px-3 text-sm font-medium text-pt-muted">そのほかの情報</summary>
                <dl className="flex flex-col gap-2.5 border-t border-pt-line px-3 py-3">
                  {other.map((c) => <ReadItem key={c.header} column={c} value={v[c.header] ?? ''} updated={changedKeys.has(c.header)} />)}
                </dl>
              </details>
            )}
          </div>
        </div>

        <footer className="border-t border-pt-line bg-pt-surface px-4 py-3 pb-[max(0.75rem,env(safe-area-inset-bottom))]">
          {confirmClose ? (
            <Notice tone="warn" title="変更を保存せずに閉じますか">
              <div className="mt-2 flex flex-wrap gap-2">
                <Button onClick={() => { setConfirmClose(false); }}>編集に戻る</Button>
                <Button variant="danger" onClick={onClose}>保存せずに閉じる</Button>
              </div>
            </Notice>
          ) : (
            <div className="flex flex-col gap-2">
              {showFresh && (
                <Notice
                  tone="warn"
                  action={<Button size="sm" onClick={showLatest}>最新を見る</Button>}
                >
                  シートで内容が変わりました。
                </Notice>
              )}
              {phase.kind === 'saved' && (
                <Notice tone="ok">保存しました。</Notice>
              )}
              {phase.kind === 'failed' && (
                <Notice tone="danger">
                  保存できませんでした。入力はそのまま残っています。
                  {failures >= 2 && <span className="mt-1 block text-xs"><ContactLine prefix="続けて保存できないときは、お電話でもお伝えいただけます。" /></span>}
                </Notice>
              )}
              {busy && <p className="text-center text-xs text-pt-muted">保存中です。終わるまでお待ちください。</p>}
              {phase.kind === 'conflict' && changedColumns.some((c) => (phase.latest[c.header] ?? '') !== (base[c.header] ?? '')) && <p className="text-center text-xs text-pt-muted">上の項目を確かめて、どちらかを選んでください。</p>}
              {!dirty && !busy && phase.kind !== 'conflict' ? (
                <Button size="lg" onClick={onClose}>一覧に戻る</Button>
              ) : (
                <Button variant="primary" size="lg" disabled={!dirty || busy || phase.kind === 'conflict'} onClick={save}>
                  {busy ? '保存中…' : phase.kind === 'failed' ? 'もう一度保存' : `保存（${String(changedColumns.length)}項目を変更）`}
                </Button>
              )}
            </div>
          )}
        </footer>
      </div>
    </div>
  );
}
