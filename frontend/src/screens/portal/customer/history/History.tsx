import { currentSheet, getData, log, useData } from '../../data';
import { AppShell } from '../../ui/shell';
import { ActivityTable, exportCsv } from './ActivityTable';

export function History() {
  const d = useData();
  const entries = d.activity.filter((e) => e.customerId === d.currentCustomerId);
  return (
    <AppShell section="history" title="記録">
      <div className="flex max-w-[1100px] flex-col gap-2">
        <p className="text-xs text-pt-faint">画面での変更も、シートで直接の変更も、ここに残ります。閲覧やログインは「すべて」で見られます。</p>
        <ActivityTable
          changesToggle
          entries={entries}
          sheetId={currentSheet(d)?.id}
          onExport={(list) => {
            exportCsv(getData(), list);
            log({ kind: 'export', detail: `記録 ${String(list.length)} 件を書き出し` });
          }}
        />
      </div>
    </AppShell>
  );
}
