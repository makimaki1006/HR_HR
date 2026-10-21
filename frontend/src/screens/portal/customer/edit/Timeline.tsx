// 「この応募者の記録」。新しい順、最初は 5 件。長い文は前と後を 2 段に分ける。
import { useState } from 'react';
import type { ActivityEntry } from '../../types';
import { formatShortDateTime } from '../../ui/kit';


const shown = (v: string): string => (v === '' ? '（空）' : v);
const isLong = (a: string, b: string): boolean => a.includes('\n') || b.includes('\n') || a.length + b.length > 24;

export function Timeline({ entries, labelFor }: { entries: ActivityEntry[]; labelFor: (h: string) => string }) {
  const [all, setAll] = useState(false);
  const list = all ? entries : entries.slice(0, 5);
  if (entries.length === 0) return <p className="text-[13px] text-pt-faint">まだ記録はありません。</p>;
  return (
    <div>
      <ol className="flex flex-col">
        {list.map((e) => {
          const before = e.before ?? '';
          const after = e.after ?? '';
          return (
            <li key={e.id} className="flex gap-3 pb-3 last:pb-0">
              <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-pt-line-strong" />
              <div className="min-w-0 flex-1 text-[13px]">
                <p className="text-pt-muted">
                  <span className="pt-num">{formatShortDateTime(e.at)}</span>
                  <span className="mx-1.5 text-pt-faint">・</span>
                  <span className="text-pt-ink">{e.actorName ?? e.accountEmail}</span>
                  {e.kind === 'save_conflict' && <span className="ml-1.5 text-pt-warn">保存しなかった</span>}
                </p>
                {e.column !== undefined ? (
                  <div className="mt-0.5 break-words text-pt-ink">
                    <span className="text-pt-muted">{labelFor(e.column)}</span>
                    {before !== '' && after.startsWith(before) && after.length > before.length ? (
                      <p className="mt-0.5 whitespace-pre-wrap"><span className="mr-1 text-pt-faint">＋</span>{after.slice(before.length).trim()}</p>
                    ) : isLong(before, after) ? (
                      <div className="mt-0.5 flex flex-col gap-0.5 border-l-2 border-pt-line pl-2">
                        <p className="whitespace-pre-wrap text-pt-faint line-through">{shown(before)}</p>
                        <p className="whitespace-pre-wrap">{shown(after)}</p>
                      </div>
                    ) : (
                      <>
                        <span className="mx-1.5">{shown(before)}</span>
                        <span className="text-pt-faint">→</span>
                        <span className="ml-1.5">{shown(after)}</span>
                      </>
                    )}
                  </div>
                ) : (
                  <p className="mt-0.5 text-pt-ink">{e.detail}</p>
                )}
              </div>
            </li>
          );
        })}
      </ol>
      {entries.length > 5 && (
        <button type="button" onClick={() => { setAll((v) => !v); }} className="mt-2 inline-flex h-[44px] items-center text-[13px] text-pt-primary hover:underline sm:h-8">
          {all ? '閉じる' : `もっと見る（残り ${String(entries.length - 5)} 件）`}
        </button>
      )}
    </div>
  );
}
