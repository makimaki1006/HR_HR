// スマホの一覧。表は使わず、カードを並べる。選考状況だけはカードの中でその場で変えられる。
import { cardLayout, extraText, labelOf } from '../../columns';
import { IconClock } from '../../ui/icons';
import { cx } from '../../ui/kit';
import type { ApplicantRow, SheetConfig } from '../../types';
import { statusDot } from './model';
import { ChangeMark, changeLine, SaveProblem } from './parts';
import { cellKey, type CellSaver } from './useCellSaver';

export function MobileList({ sheet, rows, readOnly, saver, onOpen }: {
  sheet: SheetConfig;
  rows: ApplicantRow[];
  readOnly: boolean;
  saver: CellSaver;
  onOpen: (rowNumber: number) => void;
}) {
  const layout = cardLayout(sheet);
  const status = layout.status;
  const extra = layout.extra;
  if (rows.length === 0) return <p className="px-4 py-12 text-center text-[13px] text-pt-muted">条件に合う応募者はいません</p>;
  return (
    <ul className="flex flex-col gap-2 px-3 pt-1 pb-6">
      {rows.map((row) => {
        const get = (h: string | undefined) => (h ? (row.values[h] ?? '') : '');
        const name = get(layout.title?.header);
        const sub = layout.subtitle.map((c) => get(c.header)).filter((v) => v !== '');
        const key = status ? cellKey(row.rowNumber, status.header) : '';
        const st = saver.cells[key];
        const statusValue = st?.kind === 'saving' ? st.value : get(status?.header);
        const extraValue = extraText(layout, row.values);
        const label = `${name}さんの「${status ? labelOf(status) : ''}」`;
        return (
          <li key={row.rowNumber} className="overflow-hidden rounded-lg border border-pt-line bg-pt-surface shadow-pt-card" style={changeLine(row.changedSinceLastView) ? { boxShadow: `inset 2px 0 0 ${changeLine(row.changedSinceLastView) ?? ''}` } : undefined}>
            <button type="button" onClick={() => { onOpen(row.rowNumber); }} className="block w-full px-3.5 pt-3 pb-2 text-left">
              <span className="flex items-center gap-2">
                <span className="min-w-0 truncate text-[15px] font-semibold text-pt-ink">{name}</span>
                <ChangeMark kind={row.changedSinceLastView} />
              </span>
              {sub.length > 0 && <span className="mt-0.5 block truncate text-[12px] text-pt-muted"><span className="pt-num">{sub[0]}</span>{sub.slice(1).map((s) => ` · ${s}`).join('')}</span>}
            </button>
            <div className="flex items-center gap-2 px-3.5 pb-3">
              {status && (
                <label className="relative inline-flex min-w-0 items-center">
                  <span className="sr-only">{labelOf(status)}</span>
                  <span className={cx('pointer-events-none absolute left-3 h-1.5 w-1.5 rounded-full', statusDot(sheet, statusValue))} />
                  <select
                    value={statusValue}
                    disabled={readOnly || !status.writable || st?.kind === 'saving'}
                    onChange={(e) => { saver.save(row.rowNumber, status.header, row.values[status.header] ?? '', e.target.value, label); }}
                    className="h-[44px] min-h-[44px] max-w-[12rem] appearance-none rounded-full border border-pt-line-strong bg-pt-surface pr-8 pl-7 text-[14px] text-pt-ink disabled:opacity-60"
                  >
                    {!(status.options ?? []).includes(statusValue) && <option value={statusValue}>{statusValue === '' ? '未入力' : statusValue}</option>}
                    {(status.options ?? []).map((o) => <option key={o} value={o}>{o === '' ? '（空）' : o}</option>)}
                  </select>
                  <svg className="pointer-events-none absolute right-2.5 text-pt-muted" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} aria-hidden="true"><path d="M6 9l6 6 6-6" /></svg>
                </label>
              )}
              {st?.kind === 'saving' && <span className="text-[12px] text-pt-faint">保存中</span>}
              {st?.kind === 'saved' && <span className="text-[12px] text-pt-ok">保存しました</span>}
              {extra && extraValue !== '' && (
                <span className="ml-auto inline-flex shrink-0 items-center gap-1 text-[12px] text-pt-muted">
                  <IconClock size={13} />{labelOf(extra)}<span className="pt-num text-pt-ink">{extraValue}</span>
                </span>
              )}
            </div>
            {st && (st.kind === 'conflict' || st.kind === 'failed') && (
              <div className="border-t border-pt-line px-3.5 py-2.5">
                <SaveProblem
                  state={st}
                  display={(x) => (x === '' ? '（空）' : x)}
                  onTakeSheet={() => { saver.takeSheet(key); }}
                  onOverwrite={() => { saver.overwrite(key, label); }}
                  onRetry={() => { saver.retry(key, label); }}
                  onClose={() => { saver.takeSheet(key); }}
                />
              </div>
            )}
          </li>
        );
      })}
    </ul>
  );
}
