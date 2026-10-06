import type { CrmOwner } from '../../generated/CrmOwner';

/** 検索用に揃える (全角半角・大文字小文字・前後の空白の違いを吸収する) */
export function normalizeSearch(s: string): string {
  return s.normalize('NFKC').toLowerCase().trim();
}

/** 選択肢の表示。同名の人を見分けるため email、無ければ ID を添える */
export function ownerLabel(o: CrmOwner): string {
  const tail = o.email ?? `ID ${o.id}`;
  return `${o.name}(${tail})${o.archived ? ' [退職者]' : ''}`;
}

/**
 * 選択肢に出す人。名前・email・ID を空白区切りの語すべてで絞る。退職者は includeArchived のときだけ。
 * ただし選択中の人 (selectedId) は、退職者でも検索に合わなくても必ず残す (選んだ人が消えて見えなくなるのを防ぐ)。
 */
export function visibleOwners(
  owners: readonly CrmOwner[], query: string, includeArchived: boolean, selectedId: string,
): CrmOwner[] {
  const words = normalizeSearch(query).split(/\s+/).filter(Boolean);
  return owners.filter(o => {
    if (o.id === selectedId) return true;
    if (o.archived && !includeArchived) return false;
    const hay = normalizeSearch(`${o.name} ${o.email ?? ''} ${o.id}`);
    return words.every(w => hay.includes(w));
  });
}

/** owner ID → 表示名 (一覧が無い・載っていなければ undefined) */
export function ownerNameMap(owners: readonly CrmOwner[]): Map<string, string> {
  return new Map(owners.map(o => [o.id, o.name]));
}

/** 画面に出す架空の担当者 (架空サンプルのモード用。実在の人は含まない) */
export const FIXTURE_OWNERS: CrmOwner[] = [
  { id: '9001', name: '架空 一郎', email: 'ichiro@example.invalid', archived: false },
  { id: '9002', name: 'サンプル 二郎', email: 'jiro@example.invalid', archived: false },
  { id: '9003', name: '架空 一郎', email: 'ichiro2@example.invalid', archived: false },
  { id: '9004', name: 'ダミー 退職', email: null, archived: true },
];
