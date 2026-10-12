// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';
import { QueuePropertyPicker } from './QueuePropertyPicker';
import { matchesLabel } from './propertyModel';

const p = (name: string, label: string, label_en?: string): CrmCatalogProperty => ({ name, label, ...(label_en ? { label_en } : {}), property_type: 'number', field_type: 'number', options: [] });
const amount = p('amount', '金額', 'Amount');
const custom = p('bpo_hsurl', 'BPO_HSURL');
const props = [amount, p('dealname', '取引名', 'Deal Name'), custom];

afterEach(cleanup);

describe('項目の検索は日本語の表示名と元の英語名のどちらでも見つかる', () => {
  it('matchesLabel', () => {
    expect(matchesLabel(amount, '金額')).toBe(true);
    expect(matchesLabel(amount, 'amount')).toBe(true);
    expect(matchesLabel(amount, '取引')).toBe(false);
    expect(matchesLabel(custom, 'bpo_hs')).toBe(true);
    expect(matchesLabel(custom, '')).toBe(true);
  });

  it('ピッカーは「金額」でも「Amount」でも金額を出し、候補は日本語名で表示する', () => {
    render(<QueuePropertyPicker properties={props} value={null} onChange={() => undefined} />);
    const input = screen.getByRole('combobox');
    fireEvent.change(input, { target: { value: 'Amount' } });
    expect(screen.getAllByRole('option').map(o => o.textContent)).toEqual(['金額']);
    fireEvent.change(input, { target: { value: '金額' } });
    expect(screen.getAllByRole('option').map(o => o.textContent)).toEqual(['金額']);
    fireEvent.change(input, { target: { value: 'deal name' } });
    expect(screen.getAllByRole('option').map(o => o.textContent)).toEqual(['取引名']);
  });
});
