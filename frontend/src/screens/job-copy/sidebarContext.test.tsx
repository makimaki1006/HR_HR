// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen, cleanup, fireEvent, waitFor } from '@testing-library/react';
import { SidebarDeals, sidebarMode } from './SidebarContext';
import { clearCompanyDealsCache, contractLabel, listingsLabel } from './companyDeals';
import type { CompanyDeal } from './companyDeals';

vi.mock('../../api/client', async () => ({ ...(await vi.importActual<object>('../../api/client')), apiGet: vi.fn() }));
import { apiGet } from '../../api/client';

const deal = (over: Partial<CompanyDeal>): CompanyDeal => ({
  id: '1', name: '合成案件A', pipeline_id: '21596025', pipeline_label: 'リクロジ_納品管理', stage_label: '納品中', active: true,
  created_at: '2026-09-01', contract_start: '2026-04-01', contract_end: '2027-03-31',
  listings_total: 5, listings_live: 3, recent_apps: 7, recent_cost: 20000, ...over,
});

afterEach(() => { cleanup(); clearCompanyDealsCache(); vi.mocked(apiGet).mockReset(); });

describe('左の一覧の中身', () => {
  it('階層ごとに、検索・案件・求人のどれを出すか決まる', () => {
    expect(sidebarMode('home', false)).toBe('search');
    expect(sidebarMode('company', false)).toBe('deals');
    expect(sidebarMode('deal', true)).toBe('listings');
    expect(sidebarMode('listing', true)).toBe('listings');
    // 案件が分からない求人（ホームの一覧から直接開いたとき）は検索のまま。
    expect(sidebarMode('listing', false)).toBe('search');
  });
  it('案件の文言: 掲載中 X ／ 全 Y、契約期間、集計前', () => {
    expect(listingsLabel(deal({}))).toBe('掲載中 3 ／ 全 5');
    expect(listingsLabel(deal({ listings_total: null, listings_live: null }))).toBe('集計中');
    expect(contractLabel(deal({}))).toBe('2026-04-01 〜 2027-03-31');
    expect(contractLabel(deal({ contract_start: null, contract_end: null }))).toBe('—');
  });
  it('顧客では案件だけを並べ、選ぶと案件へ進む', async () => {
    vi.mocked(apiGet).mockResolvedValue({ ok: true, data: { company: { id: 'c', name: '会社' }, deals: [deal({}), deal({ id: '2', name: '合成案件B', stage_label: '契約終了', active: false, listings_total: 2, listings_live: 0 })] } });
    const open = vi.fn();
    render(<SidebarDeals companyId="c" onOpenDeal={open} />);
    const first = await screen.findByRole('button', { name: /合成案件A/ });
    expect(first.textContent).toContain('納品中 · 掲載中 3 ／ 全 5');
    expect(screen.getByRole('button', { name: /合成案件B/ }).textContent).toContain('掲載中 0 ／ 全 2');
    expect(screen.getAllByRole('listitem')).toHaveLength(2);
    fireEvent.click(first);
    await waitFor(() => { expect(open).toHaveBeenCalledWith({ id: '1', name: '合成案件A' }); });
    expect(apiGet).toHaveBeenCalledTimes(1);
  });
});
