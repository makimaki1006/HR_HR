import { useSyncExternalStore } from 'react';

/**
 * 担当者の ID → 名前。上の帯の担当の欄が読んだ一覧を覚えておき、案件のページの見出し（担当：○○）で使う。
 * 新しく読みに行かない（一覧がまだ無ければ名前を出さない）。
 */
const names = new Map<string, string>();
const listeners = new Set<() => void>();
let version = 0;

export function publishOwnerNames(owners: readonly { id: string; name: string }[]) {
  let changed = false;
  for (const owner of owners) if (owner.name && names.get(owner.id) !== owner.name) { names.set(owner.id, owner.name); changed = true; }
  if (!changed) return;
  version += 1;
  for (const listener of listeners) listener();
}
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };

/** 担当者の名前。分からないときは null。 */
export function useOwnerName(id: string | null | undefined): string | null {
  useSyncExternalStore(subscribe, () => version);
  return id ? names.get(id) ?? null : null;
}
