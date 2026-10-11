// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { PropertyPanel } from './PropertyPanel';
import { FIXTURE_CATALOG } from './usePropertyCatalog';
import { catalogIndex, DEFAULT_SELECTED, PROPS_STORAGE_KEY } from './propertyModel';
import { personalSelection, resolvePropertyLayout, useRolePropertyLayout } from './useRolePropertyLayout';
import type { RoleLayouts } from './useRolePropertyLayout';
vi.mock('../../api/client', () => ({ apiGet: vi.fn(() => Promise.resolve({ ok: true, data: { role: 'bpo', can_edit: false, layouts: { deal: ['dealname', 'bpo_10'] } } })) }));
afterEach(() => { cleanup(); localStorage.clear(); vi.restoreAllMocks(); });
const role: RoleLayouts = { role: 'bpo', can_edit: false, layouts: { deal: ['dealname', 'bpo_10'] } };
function Harness() {
  const layout = useRolePropertyLayout('live', localStorage);
  return <><output data-testid="names">{layout.selection.deals.join(',')}</output><PropertyPanel roleLayout={layout} catalog={{ phase: 'ready', catalog: FIXTURE_CATALOG, index: catalogIndex(FIXTURE_CATALOG) }} onReloadCatalog={() => { /* fixture */ }} selection={layout.selection} onApply={layout.apply} data={null} placeholder="案件を選んでください" ownerNames={new Map()} hasSelection /></>;
}
describe('role property layout', () => {
  it('uses personal, then role, then existing defaults, preserving empty arrays and order', () => {
    const personal = { deals: ['bpo_10', 'dealname'], contacts: [], companies: [] };
    expect(resolvePropertyLayout(personal, role)).toEqual(personal);
    expect(resolvePropertyLayout(null, role)).toEqual({ ...DEFAULT_SELECTED, deals: ['dealname', 'bpo_10'] });
    expect(resolvePropertyLayout(null, null)).toEqual(DEFAULT_SELECTED);
    localStorage.setItem(PROPS_STORAGE_KEY, 'broken');
    expect(resolvePropertyLayout(personalSelection(localStorage), role).deals).toEqual(['dealname', 'bpo_10']);
    expect(resolvePropertyLayout(null, { ...role, layouts: { deal: [] } }).deals).toEqual([]);
  });
  it('reflects BPO defaults in the picker, hides admin controls, resets personal settings', async () => {
    localStorage.setItem(PROPS_STORAGE_KEY, JSON.stringify({ v: 2, deals: ['amount'], contacts: [], companies: [] }));
    render(<Harness />);
    expect(screen.getByTestId('names').textContent).toBe('amount');
    fireEvent.click(screen.getByText('役割の標準に戻す'));
    await waitFor(() => { expect(screen.getByTestId('names').textContent).toBe('dealname,bpo_10'); });
    expect(localStorage.getItem(PROPS_STORAGE_KEY)).toBeNull();
    fireEvent.click(screen.getByText('表示する項目を選ぶ'));
    expect(screen.getByLabelText('表示の順番').querySelectorAll('li')[0]?.textContent).toContain('案件名');
    expect(screen.queryByText(/この並びを/)).toBeNull();
    fireEvent.click(screen.getByLabelText('案件名を下へ'));
    fireEvent.click(screen.getByText('この項目で表示する'));
    expect(screen.getByTestId('names').textContent).toBe('bpo_10,dealname');
  });
});
