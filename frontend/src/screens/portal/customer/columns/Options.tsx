import { useState } from 'react';
import type { ApplicantRow, FunnelStage } from '../../types';
import { IconGrip, IconMore, IconPlus } from '../../ui/icons';
import { Button, Select, cx } from '../../ui/kit';
import { OptionDot } from './parts';
import { STAGE_LABEL, STAGE_ORDER, countOf, strayValues, type Draft, type OptionRow } from './model';

let newId = 0;

export function OptionEditor({ draft, saved, header, rows, isStatus, onChange }: { draft: Draft; saved: Draft; header: string; rows: ApplicantRow[]; isStatus: boolean; onChange: (list: OptionRow[]) => void }) {
  const [menu, setMenu] = useState<string | null>(null);
  const [drag, setDrag] = useState<string | null>(null);
  const [focusId, setFocusId] = useState<string | null>(null);
  const list = draft.options;
  const patch = (id: string, p: Partial<OptionRow>) => { onChange(list.map((o) => (o.id === id ? { ...o, ...p } : o))); };
  const move = (from: number, to: number) => {
    if (from === to || to < 0 || to >= list.length) return;
    const next = [...list];
    const [m] = next.splice(from, 1);
    if (m) next.splice(to, 0, m);
    onChange(next);
  };
  const add = () => {
    newId += 1;
    const id = 'n' + String(newId);
    setFocusId(id);
    onChange([...list, { id, orig: null, name: '', stage: '' }]);
  };
  const stray = strayValues(saved, rows, header);
  const emptyUses = draft.emptyAt !== null ? countOf(rows, header, '') : null;

  return (
    <div>
      <ul>
        {list.map((o, i) => {
          const uses = o.orig === null ? null : countOf(rows, header, o.orig);
          const open = menu === o.id;
          return (
            <li
              key={o.id}
              onDragOver={(e) => { if (drag !== null) e.preventDefault(); }}
              onDrop={() => { const from = list.findIndex((x) => x.id === drag); move(from, i); setDrag(null); }}
              className="group border-b border-pt-line last:border-b-0"
            >
              <div className="flex flex-wrap items-center gap-x-2 gap-y-1 py-1">
                <span
                  draggable
                  onDragStart={() => { setDrag(o.id); }}
                  onDragEnd={() => { setDrag(null); }}
                  aria-hidden="true"
                  className="hidden w-4 shrink-0 cursor-grab text-pt-faint opacity-0 group-hover:opacity-100 sm:block"
                ><IconGrip size={14} /></span>
                <OptionDot index={i} stage={o.stage} />
                <input
                  value={o.name}
                  aria-label="選択肢の名前"
                  placeholder="名前"
                  autoFocus={focusId === o.id}
                  onChange={(e) => { patch(o.id, { name: e.target.value }); }}
                  className="order-1 h-9 min-w-0 flex-1 rounded border border-transparent bg-transparent px-1.5 text-[13px] hover:bg-pt-sunken focus:border-pt-select focus:bg-pt-surface focus:outline-none sm:h-8"
                />
                <span className="pt-num order-3 w-10 shrink-0 text-right text-xs text-pt-faint">{uses === null ? '新規' : `${String(uses)}人`}</span>
                <button
                  type="button"
                  aria-label="この選択肢の操作"
                  aria-expanded={open}
                  onClick={() => { setMenu(open ? null : o.id); }}
                  className="order-4 flex h-11 w-9 shrink-0 items-center justify-center rounded text-pt-muted hover:bg-pt-sunken sm:h-8"
                ><IconMore size={16} /></button>
                {isStatus && (
                  <div className="order-5 flex basis-full items-center gap-2 pb-1 pl-9 sm:order-2 sm:basis-auto sm:pb-0 sm:pl-0">
                    <span className="shrink-0 text-xs text-pt-faint sm:hidden">数字の段階</span>
<div className="w-36 sm:w-28">                    <Select
                      value={o.stage}
                      aria-label="数字の段階"
                      className="h-9 w-full text-xs sm:h-8"
                      onChange={(e) => { patch(o.id, { stage: e.target.value as FunnelStage | '' }); }}
                    >
                      <option value="">未設定</option>
                      {STAGE_ORDER.map((s) => <option key={s} value={s}>{STAGE_LABEL[s]}</option>)}
                    </Select></div>
                  </div>
                )}
              </div>
              {open && (
                <div className="flex gap-1 pb-2 pl-6">
                  <Button size="sm" className="h-11 sm:h-7" disabled={i === 0} onClick={() => { move(i, i - 1); }}>上へ</Button>
                  <Button size="sm" className="h-11 sm:h-7" disabled={i === list.length - 1} onClick={() => { move(i, i + 1); }}>下へ</Button>
                  <Button size="sm" className="h-11 text-pt-danger sm:h-7" onClick={() => { setMenu(null); onChange(list.filter((x) => x.id !== o.id)); }}>削除</Button>
                </div>
              )}
            </li>
          );
        })}
      </ul>
      {emptyUses !== null && (
        <p className={cx('flex items-center gap-2 border-t border-pt-line py-2 text-xs text-pt-faint', list.length === 0 && 'border-t-0')}>
          <span className="h-3 w-3 shrink-0 rounded-[4px] border border-dashed border-pt-line-strong" aria-hidden="true" />
          未入力<span className="pt-num ml-auto">{emptyUses}人</span>
        </p>
      )}
      {stray.length > 0 && (
        <div className="border-t border-pt-line py-2">
          {stray.map((s) => <p key={s.value} className="text-xs text-pt-warn">一覧に無い値「{s.value}」<span className="pt-num">{s.uses}</span>人</p>)}
        </div>
      )}
      <Button size="sm" className="mt-2" onClick={add}><IconPlus size={14} />選択肢を足す</Button>
    </div>
  );
}
