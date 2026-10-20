// スマホの一覧のカード 1 枚の中身。顧客の一覧と、社内の「列の設定」のプレビューで同じものを使う。
import { Badge } from '../../ui/kit';
import { cardLayout, labelOf, type CardLayout } from '../../columns';
import type { ApplicantRow } from '../../types';

export function statusTone(s: string): 'warn' | 'info' | 'ok' | 'neutral' {
  if (s === '未対応') return 'warn';
  if (s === '内定' || s === '採用') return 'ok';
  if (s === '不採用' || s === '辞退') return 'neutral';
  return 'info';
}

export function StatusBadge({ value }: { value: string }) {
  if (value === '') return <span className="text-pt-faint">未入力</span>;
  return <Badge tone={statusTone(value)}>{value}</Badge>;
}

export function ChangeBadge({ kind }: { kind: ApplicantRow['changedSinceLastView'] }) {
  if (kind === 'new') return <Badge tone="new">新着</Badge>;
  if (kind === 'updated') return <Badge tone="warn">更新</Badge>;
  return null;
}

export function CardBody({ layout, values, changed = null }: { layout: CardLayout; values: Record<string, string>; changed?: ApplicantRow['changedSinceLastView'] }) {
  const get = (h: string | undefined) => (h ? (values[h] ?? '') : '');
  const subtitle = layout.subtitle.map((c) => get(c.header)).filter((v) => v !== '').join(' / ');
  const extraValue = get(layout.extra?.header);
  return (
    <>
      <span className="flex items-center gap-2">
        <span className="min-w-0 truncate text-base font-semibold text-pt-ink">{get(layout.title?.header)}</span>
        <ChangeBadge kind={changed} />
        {layout.status && <span className="ml-auto shrink-0"><StatusBadge value={get(layout.status.header)} /></span>}
      </span>
      {subtitle !== '' && <span className="mt-1 block text-xs text-pt-muted">{subtitle}</span>}
      {layout.extra && extraValue !== '' && (
        <span className="mt-1 block text-sm text-pt-ink">{labelOf(layout.extra)} {extraValue}</span>
      )}
    </>
  );
}

export { cardLayout };
