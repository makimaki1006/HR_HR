// スマホの一覧。表は使わず、カードを 2 段で並べる。選考状況は札を押すと下からのシートで選べる。
import { useState } from 'react';
import { cardLayout, formatDateTimeJa, formatDayJa, labelOf, TODAY_NUM } from '../../columns';
import { IconChevronDown } from '../../ui/icons';
import { BottomSheet, cx } from '../../ui/kit';
import type { ApplicantRow, ColumnConfig, SheetConfig } from '../../types';
import { rowRefDay, sheetBaseRef, statusDot } from './model';
import { interviewDayNum } from './quick';
import { ChangeMark, changeLine, SaveProblem } from './parts';
import { StatusSheetList } from './StatusSheet';
import { cellKey, type CellSaver } from './useCellSaver';

/** 2 段目の文。上から順に当てはまる 1 つだけ出す。 */
function subline(sheet: SheetConfig, row: ApplicantRow, media: ColumnConfig | undefined, action: ColumnConfig | undefined, rows: ApplicantRow[]): { lead?: string; main: string; today?: boolean } {
  const get = (h: string | undefined) => (h ? (row.values[h] ?? '') : '');
  const ref = sheet.sortColumn ? rowRefDay(sheet, row, rows) : null;
  const applied = sheet.sortColumn ? formatDayJa(get(sheet.sortColumn), sheetBaseRef(sheet, rows))?.text : undefined;
  const appliedLine = [applied !== undefined ? `${applied}応募` : '', get(media?.header)].filter((x) => x !== '').join('・');
  if (row.changedSinceLastView === 'new' && appliedLine !== '') return { main: appliedLine };
  const dayNum = interviewDayNum(sheet, row, rows);
  const stage = sheet.statusColumn ? sheet.stageMap?.[get(sheet.statusColumn)] : undefined;
  const closed = stage === 'rejected' || stage === 'declined';
  if (sheet.interviewColumn && !closed && dayNum !== null && dayNum >= TODAY_NUM) {
    const f = formatDateTimeJa(get(sheet.interviewColumn), sheet.interviewTimeColumn ? get(sheet.interviewTimeColumn) : undefined, ref);
    if (f) return { lead: '面接 ', main: f.text, today: f.isToday };
  }
  const next = get(action?.header);
  if (next !== '') return { main: next };
  return { main: appliedLine };
}

export function MobileList({ sheet, rows, allRows = rows, readOnly, saver, onOpen }: {
  sheet: SheetConfig;
  rows: ApplicantRow[];
  allRows?: ApplicantRow[];
  readOnly: boolean;
  saver: CellSaver;
  onOpen: (rowNumber: number) => void;
}) {
  const layout = cardLayout(sheet);
  const status = layout.status;
  const media = sheet.columns.find((c) => c.visible && c.header.includes('媒体'));
  const action = sheet.columns.find((c) => c.visible && c.header.includes('アクション'));
  const [picking, setPicking] = useState<number | null>(null);
  const close = () => { setPicking(null); };
  const pickRow = picking === null ? undefined : rows.find((r) => r.rowNumber === picking);
  if (rows.length === 0) return <p className="px-4 py-12 text-center text-[13px] text-pt-muted">条件に合う応募者はいません</p>;
  const nameOf = (row: ApplicantRow): string => (layout.title ? (row.values[layout.title.header] ?? '') : '');

  return (
    <>
      <ul className="flex flex-col gap-2 px-3 pt-1 pb-6">
        {rows.map((row) => {
          const name = nameOf(row);
          const key = status ? cellKey(row.rowNumber, status.header) : '';
          const st = saver.cells[key];
          const statusValue = st?.kind === 'saving' ? st.value : (status ? (row.values[status.header] ?? '') : '');
          const label = `${name}さんの「${status ? labelOf(status) : ''}」`;
          const sub = subline(sheet, row, media, action, allRows);
          const canPick = status !== undefined && status.writable && !readOnly && st?.kind !== 'saving';
          const line = changeLine(row.changedSinceLastView);
          return (
            <li key={row.rowNumber} className="relative overflow-hidden rounded-[8px] border border-pt-line bg-pt-surface shadow-pt-card" style={line ? { boxShadow: `inset 2px 0 0 ${line}` } : undefined}>
              <button type="button" aria-label={`${name}さんの詳細を開く`} onClick={() => { onOpen(row.rowNumber); }} className="absolute inset-0 z-0 h-full w-full" />
              <div className="pointer-events-none relative z-[1] px-[14px] py-3">
                <div className="flex items-center gap-2">
                  <span className="flex min-w-0 flex-1 items-center gap-2">
                    <span className="min-w-0 truncate text-base font-semibold text-pt-ink">{name}</span>
                    <ChangeMark kind={row.changedSinceLastView} />
                  </span>
                  {status && (
                    <button
                      type="button"
                      disabled={!canPick}
                      aria-label={`${label}を選ぶ（いま: ${statusValue === '' ? '未入力' : statusValue}）`}
                      onClick={() => { setPicking(row.rowNumber); }}
                      className={cx('pointer-events-auto relative inline-flex h-[28px] max-w-[45%] shrink-0 items-center gap-1.5 rounded-full border border-pt-line-strong bg-pt-surface pr-2 pl-2.5 text-[13px] text-pt-ink before:absolute before:-inset-y-2 before:-inset-x-1 before:content-[""]', !canPick && 'opacity-70')}
                    >
                      {st?.kind === 'saving' ? <span className="text-pt-faint">保存中</span> : st?.kind === 'saved' ? <span className="text-pt-ok">保存しました</span> : (
                        <>
                          <span className={cx('h-1.5 w-1.5 shrink-0 rounded-full', statusDot(sheet, statusValue))} />
                          <span className={cx('truncate', statusValue === '' && 'text-pt-faint')}>{statusValue === '' ? '未入力' : statusValue}</span>
                        </>
                      )}
                      {canPick && <IconChevronDown size={12} className="shrink-0 text-pt-muted" />}
                    </button>
                  )}
                </div>
                {sub.main !== '' && (
                  <p className="mt-1 truncate text-[13px] text-pt-muted">
                    {sub.lead}
                    {sub.lead !== undefined ? <span className={cx('pt-num text-pt-ink', sub.today === true && 'font-semibold')}>{sub.main}</span> : <span className="pt-num">{sub.main}</span>}
                  </p>
                )}
              </div>
              {st && (st.kind === 'conflict' || st.kind === 'failed') && (
                <div className="relative z-[1] border-t border-pt-line px-[14px] py-2.5">
                  <SaveProblem
                    state={st}
                    display={(x) => (x === '' ? '未入力' : x)}
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
      <BottomSheet open={pickRow !== undefined && status !== undefined} title={pickRow ? `${nameOf(pickRow)}さんの状況` : ''} onClose={close}>
        {pickRow && status && (
          <StatusSheetList
            sheet={sheet}
            options={status.options ?? []}
            value={pickRow.values[status.header] ?? ''}
            onSelect={(v) => {
              const base = pickRow.values[status.header] ?? '';
              setPicking(null);
              saver.save(pickRow.rowNumber, status.header, base, v, `${nameOf(pickRow)}さんの「${labelOf(status)}」`, `${labelOf(status)}を「${v === '' ? '未入力' : v}」にしました`);
            }}
          />
        )}
      </BottomSheet>
    </>
  );
}
