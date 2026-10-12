// @vitest-environment happy-dom
import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { CallQueueScreen } from './CallQueueScreen';
import { extraLine, extraParts } from './queueExtra';
import { makeItem, makeResponse, okCatalogFetch, okUserFetch, resetDockStorage } from './queueTestUtil';
import { catalogIndex } from './propertyModel';
import { fixtureOwnersFetch } from './useOwners';
import { FIXTURE_CATALOG } from './usePropertyCatalog';
import type { CrmPropertyCatalogResponse } from '../../generated/CrmPropertyCatalogResponse';
import type { QueueFilters } from './queueModel';

const prop = (name: string, label: string, property_type: string, field_type = 'text', options: { value: string; label: string }[] = []) =>
  ({ name, label, property_type, field_type, options: options.map(o => ({ ...o, display_order: 0, hidden: false })) });
const CATALOG: CrmPropertyCatalogResponse = { ...FIXTURE_CATALOG, objects: [{ object_type: 'deals', groups: [{ name: 'g', label: 'g', properties: [
  prop('amount', '金額', 'number', 'number'), prop('d1', '架空日付', 'date', 'date'), prop('t1', '架空日時', 'datetime', 'date'),
  prop('e1', '架空区分', 'enumeration', 'select', [{ value: 'a', label: 'Aランク' }]), prop('b1', '架空フラグ', 'bool', 'booleancheckbox'), prop('s1', '架空メモ', 'string'),
] }] }] };
const INDEX = catalogIndex(CATALOG);
const withExtra = (extra: { property: string; value: string | null }[]) => makeItem('1', { extra });

beforeEach(() => { try { window.sessionStorage.clear(); } catch { /* ignore */ } resetDockStorage(); });
afterEach(() => { cleanup(); });

describe('queue row extra values', () => {
  it('formats each property type', () => {
    const parts = extraParts(withExtra([
      { property: 'amount', value: '1080000' }, { property: 'd1', value: '2026-10-05' }, { property: 't1', value: '2026-10-05T20:30:00Z' },
      { property: 'e1', value: 'a' }, { property: 'b1', value: 'true' }, { property: 's1', value: '架空の文' },
    ]), INDEX);
    expect(extraLine(parts)).toBe('金額 1,080,000 · 架空日付 2026/10/05 · 架空日時 2026/10/06 05:30 · 架空区分 Aランク · 架空フラグ はい · 架空メモ 架空の文');
  });
  it('shows （空） for empty values, falls back to the raw option value, hides unknown properties and old responses', () => {
    expect(extraLine(extraParts(withExtra([{ property: 'amount', value: null }, { property: 'e1', value: 'zz' }, { property: 'secret_internal', value: '1' }]), INDEX)))
      .toBe('金額 （空） · 架空区分 zz');
    expect(extraParts(makeItem('2'), INDEX)).toEqual([]);
    expect(extraParts(withExtra([{ property: 'amount', value: '5' }]), null)).toEqual([]);
  });
  it('renders 金額 1,080,000 in the live list row with the full text in title', async () => {
    const fetcher = (f: QueueFilters) => Promise.resolve({ ok: true as const, data: makeResponse(f, [withExtra([{ property: 'amount', value: '1080000' }, { property: 'd1', value: null }])]) });
    render(<CallQueueScreen catalogFetcher={() => Promise.resolve({ ok: true, data: CATALOG })} userFetcher={okUserFetch} fetcher={fetcher} ownersFetcher={fixtureOwnersFetch} initialSearch="?view=queue&owner=all&sort=property:amount:desc" />);
    const line = await screen.findByTestId('queue-row-extra');
    expect(line.textContent).toBe('金額 1,080,000 · 架空日付 （空）');
    expect(line.getAttribute('title')).toBe('金額 1,080,000 · 架空日付 （空）');
  });
  it('fixture mode shows the sorted property value on each row', async () => {
    render(<CallQueueScreen catalogFetcher={okCatalogFetch} userFetcher={okUserFetch} initialSearch="?view=queue&mode=fixture&sort=property:amount:desc" />);
    await waitFor(() => { expect(screen.getAllByTestId('queue-row-extra').length).toBe(5); });
    const list = screen.getByRole('list', { name: '架電キュー' });
    const texts = within(list).getAllByTestId('queue-row-extra').map(e => e.textContent);
    for (const t of texts) expect(t).toMatch(/^金額 [\d,]+$/);
    const nums = texts.map(t => Number((t).replace(/\D/g, '')));
    expect(nums).toEqual([...nums].sort((a, b) => b - a));
  });
});
