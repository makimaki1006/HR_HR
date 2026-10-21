// スマホの一覧の先頭に出す「今日の面接」の欄。時刻の早い順に最大 3 人、電話ボタンつき。
import { useMemo } from 'react';
import { cardLayout, dayLabel, kindOf, parseTimeOfDay, TODAY_NUM } from '../../columns';
import { NOW } from '../../fixtures';
import { IconChevronRight, IconPhone } from '../../ui/icons';
import { Badge, Card, cx } from '../../ui/kit';
import type { ApplicantRow, ColumnConfig, SheetConfig } from '../../types';
import { matchesQuick, noInterviewNote, type Quick } from './quick';

const MAX_ROWS = 3;

/** 試作の「いま」の、日本時間での 0 時からの分。 */
const nowMinutes = (): number => {
  const d = new Date(Date.parse(NOW) + 9 * 3_600_000);
  return d.getUTCHours() * 60 + d.getUTCMinutes();
};

/** 面接の時刻（0 時からの分）。別の列の時間、無ければ面接の日のマスの中の時刻。読めなければ null。 */
function interviewMinutes(sheet: SheetConfig, row: ApplicantRow): number | null {
  const time = sheet.interviewTimeColumn ? parseTimeOfDay(row.values[sheet.interviewTimeColumn]) : null;
  if (time !== null) return time;
  const cell = sheet.interviewColumn ? (row.values[sheet.interviewColumn] ?? '') : '';
  return parseTimeOfDay(/\d{1,2}:\d{2}/.exec(cell)?.[0]);
}

function jobColumn(sheet: SheetConfig): ColumnConfig | undefined {
  const shown = sheet.columns.filter((c) => c.visible);
  return shown.find((c) => c.header.includes('職種')) ?? cardLayout(sheet).subtitle.find((c) => kindOf(c) === 'choice');
}

function phoneColumn(sheet: SheetConfig): ColumnConfig | undefined {
  return sheet.columns.find((c) => c.visible && kindOf(c) === 'phone');
}

export function TodayInterviews({ sheet, rows, onOpen, onPick }: {
  sheet: SheetConfig;
  /** 絞り込む前の全員。 */
  rows: ApplicantRow[];
  onOpen: (rowNumber: number) => void;
  onPick: (q: Quick) => void;
}) {
  const today = useMemo(() => {
    const list = rows.filter((r) => matchesQuick(sheet, r, 'today')).map((r) => ({ row: r, at: interviewMinutes(sheet, r) }));
    return list.sort((a, b) => {
      if (a.at === null && b.at === null) return a.row.rowNumber - b.row.rowNumber;
      if (a.at === null) return 1;
      if (b.at === null) return -1;
      return a.at - b.at;
    });
  }, [rows, sheet]);
  const tomorrow = useMemo(() => rows.filter((r) => matchesQuick(sheet, r, 'tomorrow')).length, [rows, sheet]);
  const nameCol = cardLayout(sheet).title;
  const job = jobColumn(sheet);
  const phone = phoneColumn(sheet);
  const now = nowMinutes();
  const nextIndex = today.findIndex((x) => x.at !== null && x.at > now);

  if (today.length === 0) {
    const note = noInterviewNote(sheet, rows, 'today');
    return <p className="px-4 pt-2 text-[13px] text-pt-muted">{note}</p>;
  }

  const shown = today.slice(0, MAX_ROWS);
  const rest = today.length - shown.length;
  return (
    <div className="px-3 pt-2">
      <Card className="overflow-hidden rounded-[8px]">
        <div className="flex items-baseline gap-2 px-3.5 pt-3 pb-1">
          <h2 className="text-[15px] font-semibold">今日の面接</h2>
          <span className="pt-num text-[13px] text-pt-muted">{today.length} 件</span>
          <span className="pt-num ml-auto text-[13px] text-pt-muted">{dayLabel(TODAY_NUM)}</span>
        </div>
        <ul>
          {shown.map(({ row, at }, i) => {
            const name = nameCol ? (row.values[nameCol.header] ?? '') : '';
            const jobValue = job ? (row.values[job.header] ?? '') : '';
            const tel = phone ? (row.values[phone.header] ?? '').replace(/[^\d+]/g, '') : '';
            const isNext = i === nextIndex;
            const past = at !== null && at <= now;
            return (
              <li key={row.rowNumber} className="flex h-[56px] items-center border-t border-pt-line" style={isNext ? { boxShadow: 'inset 3px 0 0 var(--color-pt-select)' } : undefined}>
                <button type="button" onClick={() => { onOpen(row.rowNumber); }} className={cx('flex h-full min-w-0 flex-1 items-center gap-3 pr-2 pl-3.5 text-left', past && 'text-pt-faint')}>
                  <span className={cx('pt-num w-[52px] shrink-0 text-[15px] font-semibold', past ? 'text-pt-faint' : 'text-pt-ink')}>{at === null ? '時間未定' : `${String(Math.floor(at / 60))}:${String(at % 60).padStart(2, '0')}`}</span>
                  <span className="min-w-0 flex-1">
                    <span className="flex items-center gap-1.5">
                      <span className={cx('truncate text-[15px]', past ? 'text-pt-faint' : 'text-pt-ink')}>{name}</span>
                      {isNext && <Badge tone="info">次</Badge>}
                    </span>
                    {jobValue !== '' && <span className="block truncate text-[13px] text-pt-faint">{jobValue}</span>}
                  </span>
                </button>
                {tel !== '' && (
                  <a href={`tel:${tel}`} aria-label={`${name}さんに電話`} className="mr-1 inline-flex h-[44px] w-[44px] shrink-0 items-center justify-center rounded-md text-pt-primary hover:bg-pt-sunken">
                    <IconPhone size={18} />
                  </a>
                )}
              </li>
            );
          })}
        </ul>
        {rest > 0 && (
          <button type="button" onClick={() => { onPick('today'); }} className="flex h-[44px] w-full items-center justify-between border-t border-pt-line px-3.5 text-[13px] text-pt-primary">
            <span>ほか <span className="pt-num">{rest}</span> 件</span><IconChevronRight size={14} />
          </button>
        )}
        {tomorrow > 0 && (
          <button type="button" onClick={() => { onPick('tomorrow'); }} className="flex h-[44px] w-full items-center justify-between border-t border-pt-line px-3.5 text-[13px] text-pt-primary">
            <span>明日 <span className="pt-num">{tomorrow}</span> 件</span><IconChevronRight size={14} />
          </button>
        )}
      </Card>
    </div>
  );
}
