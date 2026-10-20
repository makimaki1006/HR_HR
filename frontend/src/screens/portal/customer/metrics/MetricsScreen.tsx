import { useMemo, useState } from 'react';
import { currentSheet, useData } from '../../data';
import { ROUTES, usePreviewNav } from '../../preview';
import { Button, Notice, Segmented, Select, TextInput } from '../../ui/kit';
import { AppShell } from '../../ui/shell';
import { labelOf } from '../../columns';
import {
  STAGE_LABEL, STAGE_ORDER, breakdown, breakdownColumns, earliestDay, inRange, isSmall, kpis, missingStages, monthly, pct, rangesOf, tally, unmappedOptions,
  type Period,
} from './calc';
import { BreakdownTable, Chevron, Funnel, KpiCard, KpiStrip, MonthlyChart, Panel } from './parts';

const PERIODS: { value: Period; label: string }[] = [
  { value: 'this', label: '今月' },
  { value: 'last', label: '先月' },
  { value: 'last3', label: '過去 3 か月' },
  { value: 'all', label: '全部' },
  { value: 'custom', label: '期間を指定' },
];

export function MetricsScreen() {
  const d = useData();
  const nav = usePreviewNav();
  const sheet = currentSheet(d);
  const allRows = useMemo(() => (sheet ? (d.rows[sheet.id] ?? []) : []), [d.rows, sheet]);
  const [period, setPeriod] = useState<Period>('this');
  const [from, setFrom] = useState('2026-10-01');
  const [to, setTo] = useState('2026-10-20');
  const [pick, setPick] = useState('');

  const ranges = useMemo(() => rangesOf(period, from, to), [period, from, to]);
  const rows = useMemo(() => inRange(allRows, sheet?.sortColumn, ranges.cur), [allRows, sheet, ranges.cur]);
  const prevRows = useMemo(() => (ranges.prev ? inRange(allRows, sheet?.sortColumn, ranges.prev) : null), [allRows, sheet, ranges.prev]);
  const candidates = useMemo(() => (sheet ? breakdownColumns(sheet, allRows) : []), [sheet, allRows]);

  const canEditColumns = d.viewer.kind === 'staff' || sheet?.customerCanEditColumns === true;
  if (sheet?.connection !== 'ok' || !sheet.statusColumn || !sheet.stageMap) {
    const unreadable = sheet?.connection !== 'ok';
    return (
      <AppShell section="metrics" title="数字">
        <div className="mx-auto max-w-5xl">
          <Notice
            tone="warn"
            title={unreadable ? 'このシートは読み込めていません' : '選考状況の列が決まっていません'}
            action={!unreadable && canEditColumns ? <Button size="sm" onClick={() => { nav.go(ROUTES.columns); }}>項目の設定で決める</Button> : undefined}
          >
            {unreadable ? '読み込めるようになると数字が出ます。' : '選考状況の列を決めると数字が出ます。'}
          </Notice>
        </div>
      </AppShell>
    );
  }

  const t = tally(rows, sheet);
  const k = kpis(t);
  const oldest = earliestDay(allRows, sheet.sortColumn);
  const prevOk = prevRows !== null && prevRows.length > 0 && ranges.prev !== null && oldest !== null && ranges.prev.from >= oldest;
  const pk = prevOk ? kpis(tally(prevRows, sheet)) : null;
  const missing = missingStages(sheet);
  const heldNone = missing.has('interviewed');
  const key = candidates.some((c) => c.header === pick) ? pick : (candidates[0]?.header ?? '');
  const rowsBy = key ? breakdown(rows, sheet, key) : [];
  const unmapped = unmappedOptions(sheet, allRows);
  const statusLabel = labelOf(sheet.columns.find((c) => c.header === sheet.statusColumn) ?? { header: sheet.statusColumn, label: '', visible: true, writable: false, options: null, multiline: false, inList: true });
  const optionsOf = (stage: (typeof STAGE_ORDER)[number]): string[] => Object.entries(sheet.stageMap ?? {}).filter(([, s]) => s === stage).map(([o]) => o);

  return (
    <AppShell section="metrics" title="数字">
      <div className="mx-auto max-w-5xl space-y-4">
        <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
          <div className="hidden sm:block">
            <Segmented value={period} options={PERIODS} onChange={setPeriod} size="sm" />
          </div>
          <div className="w-full sm:hidden">
            <Select value={period} aria-label="期間" onChange={(e) => { setPeriod(e.target.value as Period); }}>
              {PERIODS.map((p) => <option key={p.value} value={p.value}>{p.label}</option>)}
            </Select>
          </div>
          {period === 'custom' && (
            <div className="flex items-center gap-2">
              <TextInput type="date" aria-label="開始日" value={from} onChange={(e) => { setFrom(e.target.value); }} className="w-40 sm:h-[34px]" />
              <span className="text-pt-faint">〜</span>
              <TextInput type="date" aria-label="終了日" value={to} onChange={(e) => { setTo(e.target.value); }} className="w-40 sm:h-[34px]" />
            </div>
          )}
          <p className="pt-num text-xs text-pt-muted">
            {[sheet.sortColumn ? `${sheet.sortColumn}で集計` : '', ranges.prevLabel !== '' ? (pk ? `増減は${ranges.prevLabel}との差` : '比較できる期間がありません') : ''].filter((x) => x !== '').join(' ・ ')}
          </p>
        </div>

        {t.total === 0 ? (
          <div className="rounded-lg border border-pt-line bg-pt-surface px-4 py-10 text-center text-[13px] text-pt-muted">この期間の応募はありません。期間を広げてください。</div>
        ) : null}

        <KpiStrip>
          <KpiCard label="応募数" big={String(k.count)} unit="人" cur={k.count} prev={pk ? pk.count : null} kind="count" />
          <KpiCard label="面接移行率" big={pct(k.interview.value)} num={k.interview.num} den={k.interview.den} cur={k.interview.value} prev={pk ? pk.interview.value : null} kind="rate" small={isSmall(k.interview)} />
          <KpiCard label="面接実施率" basis="面接設定から" big={pct(k.held.value)} num={k.held.num} den={k.held.den} cur={k.held.value} prev={pk ? pk.held.value : null} kind="rate" small={isSmall(k.held)} unavailable={heldNone} />
          <KpiCard label="内定率" big={pct(k.offer.value)} num={k.offer.num} den={k.offer.den} cur={k.offer.value} prev={pk ? pk.offer.value : null} kind="rate" small={isSmall(k.offer)} />
          <KpiCard label="採用率" big={pct(k.hire.value)} num={k.hire.num} den={k.hire.den} cur={k.hire.value} prev={pk ? pk.hire.value : null} kind="rate" small={isSmall(k.hire)} />
          <KpiCard label="辞退率" big={pct(k.decline.value)} num={k.decline.num} den={k.decline.den} cur={k.decline.value} prev={pk ? pk.decline.value : null} kind="rate" inverse small={isSmall(k.decline)} />
        </KpiStrip>
        {heldNone && canEditColumns && (
          <p className="-mt-2 text-xs text-pt-faint">面接実施に当たる選択肢が決まっていません。<button type="button" className="text-pt-primary hover:underline" onClick={() => { nav.go(ROUTES.columns); }}>項目の設定で決める</button></p>
        )}

        <div className="grid gap-4 lg:grid-cols-2">
          <Panel title="選考の流れ" aside={<span className="text-xs text-pt-faint">割合は応募数に対して</span>}><Funnel t={t} missing={missing} /></Panel>
          <Panel title="月ごとの応募数と採用数"><MonthlyChart bars={monthly(allRows, sheet)} range={ranges.cur} /></Panel>
        </div>

        <Panel
          title="応募者の割合"
          aside={candidates.length > 0 ? (
            <label className="flex items-center gap-2 text-xs whitespace-nowrap text-pt-muted">
              内訳の項目
              <Select value={key} onChange={(e) => { setPick(e.target.value); }} className="w-40">
                {candidates.map((c) => <option key={c.header} value={c.header}>{labelOf(c)}</option>)}
              </Select>
            </label>
          ) : undefined}
        >
          {candidates.length === 0 ? <p className="text-[13px] text-pt-muted">内訳に使える項目がありません。</p> : <BreakdownTable rows={rowsBy} />}
        </Panel>

        <details className="group rounded-lg border border-pt-line bg-pt-surface">
          <summary className="flex h-11 cursor-pointer items-center gap-2 px-4 text-[13px] font-medium select-none sm:h-10">
            数字の決め方
            {unmapped.length > 0 && <span className="text-xs font-normal text-pt-warn">数えていない選択肢があります</span>}
            <Chevron />
          </summary>
          <div className="space-y-3 border-t border-pt-line px-4 py-3">
            <p className="text-xs text-pt-muted">「{statusLabel}」の選択肢を、次の段階として数えています。面接以降の人数には、不採用・辞退の人は含めません。</p>
            <table className="w-full text-[13px]">
              <tbody>
                {STAGE_ORDER.map((s) => (
                  <tr key={s} className="border-b border-pt-line last:border-b-0 align-top">
                    <th scope="row" className="w-24 py-1.5 pr-3 text-left font-medium">{STAGE_LABEL[s]}</th>
                    <td className="py-1.5 text-pt-muted">{optionsOf(s).join('、') || '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {unmapped.length > 0 && <p className="text-xs text-pt-warn">数えていない選択肢: {unmapped.join('、')}</p>}
            {canEditColumns && <Button size="sm" variant="ghost" className="-ml-2.5" onClick={() => { nav.go(ROUTES.columns); }}>項目の設定で変える</Button>}
          </div>
        </details>
      </div>
    </AppShell>
  );
}
