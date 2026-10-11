import { useCallback, useEffect, useMemo, useState } from 'react';
import { apiGet } from '../../api/client';
import { DEFAULT_SELECTED, LEGACY_PROPS_STORAGE_KEY, PROPS_STORAGE_KEY, loadSelected, migrateLegacySelected, parsePersonalSelected, saveSelected } from './propertyModel';
import type { CatalogObject, SelectedProps } from './propertyModel';
import type { QueueMode } from './queueModel';
export const ROLE_LABELS = { admin: '管理者', consultant: '営業・コンサルタント', bpo: 'BPO', user: '一般' };
export type LayoutRole = keyof typeof ROLE_LABELS;
export interface RoleLayouts { role: LayoutRole; can_edit: boolean; layouts: Partial<Record<'deal' | 'contact' | 'company', string[]>> }
export const SINGULAR: Record<CatalogObject, 'deal' | 'contact' | 'company'> = { deals: 'deal', contacts: 'contact', companies: 'company' };
export function personalSelection(storage: Storage | null): SelectedProps | null {
  try {
    if (!storage) return null;
    const current = storage.getItem(PROPS_STORAGE_KEY);
    if (current !== null) return parsePersonalSelected(current);
    return migrateLegacySelected(storage.getItem(LEGACY_PROPS_STORAGE_KEY)) === null ? null : loadSelected(storage);
  } catch { return null; }
}
export function resolvePropertyLayout(personal: SelectedProps | null, role: RoleLayouts | null): SelectedProps {
  return personal ?? { deals: role?.layouts.deal ?? DEFAULT_SELECTED.deals, contacts: role?.layouts.contact ?? DEFAULT_SELECTED.contacts, companies: role?.layouts.company ?? DEFAULT_SELECTED.companies };
}
export function useRolePropertyLayout(mode: QueueMode, storage: Storage | null, enabled = true) {
  const [personal, setPersonal] = useState(() => personalSelection(storage));
  const [role, setRole] = useState<RoleLayouts | null>(null);
  useEffect(() => {
    if (mode !== 'live' || !enabled) return;
    const controller = new AbortController();
    void apiGet<RoleLayouts>('/api/crm/property-layouts', { signal: controller.signal }).then(result => { if (result.ok && !controller.signal.aborted) setRole(result.data); });
    return () => { controller.abort(); };
  }, [mode, enabled]);
  const apply = useCallback((next: SelectedProps) => { setPersonal(next); saveSelected(storage, next); }, [storage]);
  const reset = useCallback(() => { try { storage?.removeItem(PROPS_STORAGE_KEY); storage?.removeItem(LEGACY_PROPS_STORAGE_KEY); } catch { /* browser may deny storage */ } setPersonal(null); }, [storage]);
  const saveStandard = useCallback(async (target: LayoutRole, object: CatalogObject, properties: string[]) => {
    const response = await fetch(`/api/crm/property-layouts/${target}/${SINGULAR[object]}`, { method: 'PUT', credentials: 'same-origin', signal: AbortSignal.timeout(15_000), headers: { 'Content-Type': 'application/json', 'X-Requested-With': 'fetch' }, body: JSON.stringify({ properties }) });
    if (!response.ok) throw new Error('保存できませんでした。');
    if (target === role?.role) setRole(previous => previous ? { ...previous, layouts: { ...previous.layouts, [SINGULAR[object]]: properties } } : previous);
  }, [role]);
  const selection = useMemo(() => resolvePropertyLayout(personal, mode === 'live' ? role : null), [personal, role, mode]);
  return useMemo(() => ({ selection, apply, reset, role: mode === 'live' ? role : null, saveStandard,
    ordered: personal !== null || (mode === 'live' && role !== null && Object.keys(role.layouts).length > 0) }), [selection, apply, reset, mode, role, saveStandard, personal]);
}
