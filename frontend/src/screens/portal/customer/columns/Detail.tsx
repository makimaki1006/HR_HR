import { useEffect, useImperativeHandle, useRef, useState, type Ref } from 'react';
import { actions, getData } from '../../data';
import { GROUP_LABEL, GROUP_ORDER, KIND_LABEL, KIND_ORDER, groupOf, kindOf } from '../../columns';
import { ROUTES, usePreviewNav } from '../../preview';
import type { ApplicantRow, ColumnConfig, SheetConfig } from '../../types';
import { IconCheck, IconChevronRight, IconUndo } from '../../ui/icons';
import { Button, Dialog, Segmented, Select, TextInput, cx } from '../../ui/kit';
import { OptionEditor } from './Options';
import { Chip, KIND_ICON, OptionDot, SettingRow, Toggle, useEscape } from './parts';
import { STAGE_LABEL, countOf, deletedOf, impactsOf, makeDraft, namesOf, optionDiff, sameDraft, stageDiff, stageMapOf, validate, type Draft, type Impact } from './model';

interface Choice { mode: 'keep' | 'replace'; target: string }
interface Skipped { rowNumber: number; name: string }
interface Result { replaced: number; conflicts: Skipped[]; failed: Skipped[] }

export interface DetailHandle {
  /** 保存を始める。済んだら after を呼ぶ（取り消されたら呼ばない）。 */
  save: (after: () => void) => void;
  discard: () => void;
}

interface Props {
  sheet: SheetConfig;
  col: ColumnConfig;
  rows: ApplicantRow[];
  canEdit: boolean;
  isStaff: boolean;
  onBack: () => void;
  onDirty: (dirty: boolean) => void;
  onToggleVisible: (c: ColumnConfig) => void;
  ref?: Ref<DetailHandle>;
}

const nameOfCol = (c: ColumnConfig): string => (c.label.trim() !== '' ? c.label.trim() : c.header);

export function Detail({ sheet, col, rows, canEdit, isStaff, onBack, onDirty, onToggleVisible, ref }: Props) {
  const nav = usePreviewNav();
  const isStatus = sheet.statusColumn === col.header;
  const [base, setBase] = useState(() => makeDraft(col, sheet));
  const [draft, setDraft] = useState(base);
  const [pending, setPending] = useState<{ impacts: Impact[]; choices: Record<string, Choice> } | null>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<Result | null>(null);
  const afterRef = useRef<(() => void) | null>(null);

  const dirty = !sameDraft(draft, base);
  const error = validate(draft);
  useEffect(() => { onDirty(dirty); }, [dirty, onDirty]);
  useEffect(() => () => { onDirty(false); }, [onDirty]);

  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => { setDraft((d) => ({ ...d, [k]: v })); setResult(null); };
  const cancelPending = () => { setPending(null); afterRef.current = null; };
  useEscape(pending !== null, cancelPending);

  async function apply(choices: Record<string, Choice>) {
    setBusy(true);
    setPending(null);
    const latestCols = getData().sheets.find((s) => s.id === sheet.id)?.columns ?? [];
    const patch: Partial<ColumnConfig> = {};
    const parts: string[] = [];
    if (draft.label.trim() !== base.label.trim()) { patch.label = draft.label.trim(); parts.push(draft.label.trim() === '' ? '画面での名前をシートの見出しに戻した' : `画面での名前を「${draft.label.trim()}」に変更`); }
    if (draft.kind !== base.kind) { patch.kind = draft.kind; parts.push(`種類を「${KIND_LABEL[draft.kind]}」に変更`); }
    if (draft.group !== base.group) { patch.group = draft.group; parts.push(`まとまりを「${GROUP_LABEL[draft.group]}」に変更`); }
    if (draft.inList !== base.inList) { patch.inList = draft.inList; parts.push(draft.inList ? '一覧に出す' : '一覧に出さない'); }
    if (isStaff && draft.writable !== base.writable) { patch.writable = draft.writable; parts.push(draft.writable ? '書き換えを許可' : '書き換えを止める'); }
    if (draft.multiline !== base.multiline) { patch.multiline = draft.multiline; parts.push(draft.multiline ? '複数行にした' : '1 行にした'); }
    const od = draft.kind === 'choice' ? optionDiff(base, draft) : [];
    if (draft.kind !== 'choice') { if (draft.kind !== base.kind) patch.options = null; }
    else if (od.length > 0 || draft.kind !== base.kind) patch.options = namesOf(draft);
    if (od.length > 0) parts.push(`選択肢 ${od.join('、')}`);
    if (isStatus) {
      const sd = stageDiff(base, draft);
      const next = stageMapOf(draft);
      const cur = getData().sheets.find((s) => s.id === sheet.id)?.stageMap ?? {};
      if (JSON.stringify(Object.entries(next).sort()) !== JSON.stringify(Object.entries(cur).sort())) actions.updateSheet(sheet.id, { stageMap: next });
      if (sd.length > 0) parts.push(`数字の段階 ${sd.join('、')}`);
    }
    const title = sheet.titleColumn ?? '';
    const targets: { r: ApplicantRow; orig: string; to: string }[] = [];
    for (const [orig, c] of Object.entries(choices)) {
      if (c.mode !== 'replace') continue;
      for (const r of rows.filter((x) => (x.values[col.header] ?? '') === orig)) targets.push({ r, orig, to: c.target });
    }
    if (targets.length > 0) parts.push(`${String(targets.length)}人の値を置き換え`);
    if (parts.length > 0) {
      actions.updateColumns(sheet.id, latestCols.map((c) => (c.header === col.header ? { ...c, ...patch } : c)), `「${nameOfCol({ ...col, ...patch })}」: ${parts.join('、')}`);
    }
    const res = await Promise.all(targets.map((t) => actions.saveCell(sheet.id, t.r.rowNumber, col.header, t.orig, t.to, { replacement: true })));
    const out: Result = { replaced: 0, conflicts: [], failed: [] };
    res.forEach((x, i) => {
      const t = targets[i];
      if (!t) return;
      const who = { rowNumber: t.r.rowNumber, name: t.r.values[title] ?? '' };
      if (x.kind === 'saved') out.replaced += 1;
      else if (x.kind === 'conflict') out.conflicts.push(who);
      else out.failed.push(who);
    });
    const freshSheet = getData().sheets.find((s) => s.id === sheet.id) ?? sheet;
    const nb = makeDraft(freshSheet.columns.find((c) => c.header === col.header) ?? col, freshSheet);
    setBase(nb);
    setDraft(nb);
    setBusy(false);
    setResult(out);
    const after = afterRef.current;
    afterRef.current = null;
    if (out.conflicts.length === 0 && out.failed.length === 0) after?.();
  }

  const begin = () => {
    const impacts = draft.kind === 'choice' ? [...impactsOf(draft, rows, col.header), ...deletedOf(draft, col, rows)] : [];
    if (impacts.length === 0) { void apply({}); return; }
    const remaining = draft.options.map((o) => o.name);
    const choices: Record<string, Choice> = {};
    for (const i of impacts) choices[i.orig] = i.type === 'rename' ? { mode: 'replace', target: i.newName } : { mode: 'keep', target: remaining[0] ?? '' };
    setPending({ impacts, choices });
  };

  useImperativeHandle(ref, () => ({
    save: (after) => { afterRef.current = after; begin(); },
    discard: () => { setDraft(base); setResult(null); },
  }));

  const kindNow = draft.kind;
  const optionEditable = canEdit && kindNow === 'choice';
  const unsetStages = isStatus ? draft.options.filter((o) => o.stage === '').length : 0;
  const title = nameOfCol(col);

  const actionsBar = canEdit && (
    <>
      <Button variant="primary" disabled={!dirty || error !== null || busy} onClick={begin}>{busy ? '保存しています' : '保存'}</Button>
      <Button disabled={!dirty || busy} onClick={() => { setDraft(base); setResult(null); }}><IconUndo size={14} />取り消し</Button>
    </>
  );

  return (
    <div className="max-w-[560px] rounded-lg border border-pt-line bg-pt-surface pb-16 sm:pb-0">
      <div className="flex items-center gap-2 border-b border-pt-line px-4 py-3">
        <button type="button" onClick={onBack} className="-ml-1 flex h-11 items-center rounded px-1 text-[13px] text-pt-muted hover:bg-pt-sunken lg:hidden" aria-label="項目の一覧へ戻る">
          <IconChevronRight size={14} className="rotate-180" />一覧
        </button>
        <div className="min-w-0 flex-1">
          <h2 className="flex items-center gap-1.5 truncate text-[15px] font-semibold">{title}{dirty && <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-pt-warn" aria-label="保存していない変更があります" />}</h2>
          {col.label.trim() !== '' && col.label.trim() !== col.header && <p className="truncate text-xs text-pt-faint">シートの見出し: {col.header}</p>}
        </div>
      </div>

      <div className="px-4">
        {canEdit ? (
          <>
            <SettingRow label="画面での名前" hint="空ならシートの見出し">
              <TextInput value={draft.label} placeholder={col.header} className="h-8 w-48 text-[13px]" onChange={(e) => { set('label', e.target.value); }} />
            </SettingRow>
            <SettingRow label="種類">
              {isStatus ? <span className="text-[13px] text-pt-muted">選択肢（選考状況）</span> : KIND_ORDER.map((k) => {
                const Icon = KIND_ICON[k];
                return <Chip key={k} active={kindNow === k} onClick={() => { set('kind', k); }}><Icon size={12} />{KIND_LABEL[k]}</Chip>;
              })}
            </SettingRow>
            <SettingRow label="まとまり">
              {GROUP_ORDER.map((g) => <Chip key={g} active={draft.group === g} onClick={() => { set('group', g); }}>{GROUP_LABEL[g]}</Chip>)}
            </SettingRow>
            <SettingRow label="この項目を画面に出す" hint="すぐ反映されます"><Toggle label="この項目を画面に出す" checked={col.visible} onChange={() => { onToggleVisible(col); }} /></SettingRow>
            <SettingRow label="一覧（スマホのカード）にも出す"><Toggle label="一覧にも出す" checked={draft.inList} disabled={!col.visible} onChange={(v) => { set('inList', v); }} /></SettingRow>
            <SettingRow label="画面から書き換えられる" hint={isStaff ? undefined : '弊社が決めます'}><Toggle label="画面から書き換えられる" checked={draft.writable} disabled={!isStaff} onChange={(v) => { set('writable', v); }} /></SettingRow>
            {kindNow === 'text' && <SettingRow label="長い文章（複数行で入力）"><Toggle label="複数行" checked={draft.multiline} onChange={(v) => { set('multiline', v); }} /></SettingRow>}
          </>
        ) : (
          <>
            <SettingRow label="種類"><span className="text-[13px]">{KIND_LABEL[kindOf(col)]}</span></SettingRow>
            <SettingRow label="まとまり"><span className="text-[13px]">{GROUP_LABEL[groupOf(col)]}</span></SettingRow>
            <SettingRow label="画面に出す"><span className="text-[13px]">{col.visible ? 'はい' : 'いいえ'}</span></SettingRow>
            <SettingRow label="一覧に出す"><span className="text-[13px]">{col.inList ? 'はい' : 'いいえ'}</span></SettingRow>
            <SettingRow label="書き換え"><span className="text-[13px]">{col.writable ? 'できる' : 'できない'}</span></SettingRow>
          </>
        )}

        {kindNow === 'choice' && (
          <div className="border-t border-pt-line py-3">
            <div className="mb-1 flex items-center gap-2">
              <p className="text-[13px] font-medium">プルダウンの選択肢</p>
              <span className="pt-num text-xs text-pt-faint">{draft.options.length}</span>
              {isStatus && <button type="button" onClick={() => { nav.go(ROUTES.metrics); }} className="ml-auto text-xs text-pt-primary hover:underline">数字の画面を見る</button>}
            </div>
            {optionEditable ? (
              <>
                <OptionEditor draft={draft} saved={base} header={col.header} rows={rows} isStatus={isStatus} onChange={(list) => { set('options', list); }} />
                <p className="mt-2 text-xs text-pt-faint">シートのプルダウン（入力規則）も同じに変わります</p>
                {unsetStages > 0 && <p className="mt-1 text-xs text-pt-warn">段階が未設定の選択肢が {unsetStages} 個あります。数字には入りません。</p>}
              </>
            ) : (
              <table className="w-full text-[13px]">
                <tbody>
                  {draft.options.map((o, i) => (
                    <tr key={o.id} className="border-b border-pt-line last:border-b-0">
                      <td className="py-2"><span className="flex items-center gap-2"><OptionDot index={i} stage={o.stage} />{o.name}</span></td>
                      <td className="pt-num w-14 py-2 text-right text-xs text-pt-faint">{countOf(rows, col.header, o.orig ?? '')}人</td>
                      {isStatus && <td className="w-24 py-2 text-right text-xs text-pt-muted">{o.stage === '' ? '未設定' : STAGE_LABEL[o.stage]}</td>}
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}
      </div>

      {canEdit && (
        <div className="border-t border-pt-line px-4 py-3">
          <div className="hidden flex-wrap items-center gap-2 sm:flex">{actionsBar}</div>
          {error && <p className="text-xs text-pt-danger sm:mt-2">{error}</p>}
          {!error && result === null && <p className="text-xs text-pt-faint sm:mt-2">保存すると記録に残ります</p>}
          {result && (
            <div className="sm:mt-2">
              <p className={cx('inline-flex items-center gap-1 text-xs', result.conflicts.length + result.failed.length > 0 ? 'text-pt-warn' : 'text-pt-ok')}>
                {result.conflicts.length + result.failed.length === 0 && <IconCheck size={14} />}
                {result.replaced > 0 ? `保存しました。${String(result.replaced)}人の値を置き換えました。` : '保存しました。'}
                {result.conflicts.length > 0 && `${String(result.conflicts.length)}人はシートで先に変更されていたため置き換えていません。`}
                {result.failed.length > 0 && `${String(result.failed.length)}人は保存できませんでした。`}
              </p>
              {result.conflicts.length + result.failed.length > 0 && (
                <details className="mt-1 text-xs text-pt-muted">
                  <summary className="cursor-pointer">置き換えていない人</summary>
                  <ul className="mt-1 flex flex-wrap gap-x-3">
                    {[...result.conflicts, ...result.failed].map((s) => (
                      <li key={s.rowNumber}><button type="button" className="text-pt-primary hover:underline" onClick={() => { nav.go(ROUTES.list); }}>{s.name}</button></li>
                    ))}
                  </ul>
                </details>
              )}
            </div>
          )}
        </div>
      )}
      {canEdit && dirty && (
        <div className="fixed inset-x-0 bottom-14 z-30 flex items-center gap-2 border-t border-pt-line bg-pt-surface px-4 py-2 shadow-pt-pop sm:hidden">{actionsBar}<span className="ml-1 text-xs text-pt-warn">保存していない変更</span></div>
      )}

      <Dialog
        open={pending !== null}
        title="すでに使われている選択肢"
        onClose={cancelPending}
        footer={<><Button onClick={cancelPending}>戻る</Button><Button variant="primary" onClick={() => { if (pending) void apply(pending.choices); }}>この内容で保存</Button></>}
      >
        {pending && (
          <div className="flex flex-col gap-4">
            {pending.impacts.map((i) => {
              const c = pending.choices[i.orig] ?? { mode: 'keep' as const, target: '' };
              const upd = (n: Choice) => { setPending({ ...pending, choices: { ...pending.choices, [i.orig]: n } }); };
              return (
                <div key={i.orig} className="flex flex-col gap-2 border-b border-pt-line pb-4 last:border-b-0 last:pb-0">
                  <p className="text-[13px]"><span className="font-medium">「{i.orig}」</span>を{i.type === 'rename' ? `「${i.newName}」に変えます` : '消します'}。<span className="pt-num">{i.uses}</span>人の値をどうしますか。</p>
                  <Segmented<'keep' | 'replace'>
                    value={c.mode}
                    onChange={(m) => { upd({ ...c, mode: m }); }}
                    options={[{ value: 'keep', label: 'そのまま残す' }, { value: 'replace', label: i.type === 'rename' ? '新しい名前に置き換える' : '別の選択肢に置き換える' }]}
                  />
                  {c.mode === 'keep' && <p className="text-xs text-pt-muted">{i.uses}人の値はプルダウンに無い値として残ります</p>}
                  {c.mode === 'replace' && i.type === 'delete' && (
                    <Select value={c.target} aria-label="置き換え先" onChange={(e) => { upd({ ...c, target: e.target.value }); }}>
                      {draft.options.map((o) => <option key={o.id} value={o.name}>{o.name}</option>)}
                    </Select>
                  )}
                </div>
              );
            })}
            <p className="text-xs text-pt-muted">置き換えた分は、応募者ごとに記録に残ります。</p>
          </div>
        )}
      </Dialog>
    </div>
  );
}
