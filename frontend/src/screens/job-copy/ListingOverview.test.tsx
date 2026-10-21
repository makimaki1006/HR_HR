// @vitest-environment happy-dom
import { beforeEach as pinSidebarBeforeEach, afterEach as clearSidebarAfterEach } from 'vitest';
pinSidebarBeforeEach(() => { localStorage.setItem('hrhr-job-copy-sidebar-pinned', 'true'); });
clearSidebarAfterEach(() => { localStorage.removeItem('hrhr-job-copy-sidebar-pinned'); });
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { JobCopyScreen } from './JobCopyScreen';

vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
beforeEach(() => { window.history.replaceState(null, '', '/app/job-copy?demo=1'); });
afterEach(() => { cleanup(); });
function query(selector: string): HTMLElement {
  const element = document.querySelector<HTMLElement>(selector);
  if (!element) throw new Error(`${selector} が見つかりません`);
  return element;
}

describe('求人の画面（案3-2）', () => {
  it('上の帯に最新の出来事と、全文を比べる・新しいパターンを作る操作がある', () => {
    render(<JobCopyScreen />);
    const band = screen.getByRole('region', { name: 'この案件の要点' });
    expect(within(band).getByTestId('band-latest-event').textContent).toBe('最新の出来事9/25 給与と本文と画像を変更');
    expect(within(band).getByRole('button', { name: '求人の全文を比べる' })).toBeTruthy();
    expect(within(band).getByRole('button', { name: 'この求人から新しいパターンを作る' })).toBeTruthy();
  });

  it('出来事は応募も含めて新しい順に9件、番号は古い順。件数は前の4週 → 後の4週で、判定語を出さない', () => {
    render(<JobCopyScreen />);
    const list = query('.lo-events');
    const heads = list.querySelectorAll('.lo-event-head');
    expect([...heads].map(head => head.querySelector('.lo-num')?.textContent)).toEqual(['9', '応', '応', '6', '5', '応', '応', '2', '1']);
    expect(heads[0]?.querySelector('.lo-result strong')?.textContent).toBe('15 → 3 件');
    expect(heads[0]?.querySelector('.lo-result small')?.textContent).toBe('差 −12 件（前は掲載後24日分・後は11日分）');
    expect(list.textContent).not.toMatch(/増えた|減った|効果|効いた/u);
  });

  it('出来事を押すと、ほかに応募を動かしそうなことが出る。もう一度押すと閉じる', () => {
    render(<JobCopyScreen />);
    const head = query('.lo-event-head');
    expect(head.getAttribute('aria-expanded')).toBe('false');
    fireEvent.click(head);
    expect(head.getAttribute('aria-expanded')).toBe('true');
    const checks = screen.getByRole('list', { name: 'ほかに応募を動かしそうなこと' });
    expect(within(checks).getAllByRole('listitem').map(item => item.querySelector('strong')?.textContent)).toEqual(['課金', '市場', 'ほかの変更']);
    expect(within(checks).getByText('「課金を開始」が24日前（9/01）に、「課金を増額」が10日前（9/15）にあります。')).toBeTruthy();
    fireEvent.click(head);
    expect(head.getAttribute('aria-expanded')).toBe('false');
  });

  it('出来事を押すと、横の時間軸のその週に印が付く', () => {
    render(<JobCopyScreen />);
    expect(screen.queryByTestId('lifeline-selected')).toBeNull();
    fireEvent.click(query('.lo-event-head'));
    const mark = screen.getByTestId('lifeline-selected');
    expect(mark.style.width).toBe('56px');
    expect(query('.lo-event.lo-open .lo-num').textContent).toBe('9');
  });

  it('応募者・応募理由の前後は横から出て、閉じられる。「ほかの分析」は入口の1つから開き、タブは6つ', () => {
    render(<JobCopyScreen />);
    fireEvent.click(query('.lo-event-head'));
    fireEvent.click(screen.getByRole('button', { name: '応募者・応募理由の前後を見る' }));
    const drawer = screen.getByRole('dialog', { name: '応募者・応募理由の前後' });
    expect(within(drawer).getByText(/前の4週 15 件 → 後の4週 3 件/u)).toBeTruthy();
    fireEvent.click(within(drawer).getByRole('button', { name: '閉じる' }));
    expect(screen.queryByRole('dialog', { name: '応募者・応募理由の前後' })).toBeNull();
    const more = within(screen.getByRole('region', { name: 'ほかの分析' }));
    expect(more.getAllByRole('button').map(button => button.textContent)).toEqual(['ほかの分析を開く']);
    fireEvent.click(more.getByRole('button', { name: 'ほかの分析を開く' }));
    expect(within(screen.getByRole('tablist', { name: '求人管理の機能' })).getAllByRole('tab').map(tab => tab.textContent)).toEqual(['タイムライン', '本文・画像', '変更差分', '求人比較', '応募者構成', '応募理由']);
  });
});
