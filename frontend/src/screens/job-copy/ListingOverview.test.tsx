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

  it('出来事は既定で新しい順に5件だけ出し、すべて表示で全9件になる。番号は古い順で、判定語を出さない', () => {
    render(<JobCopyScreen />);
    const nums = () => [...query('.lo-events').querySelectorAll('.lo-event-head')].map(head => head.querySelector('.lo-num')?.textContent);
    expect(nums()).toEqual(['9', '応', '応', '6', '5']);
    expect(screen.getByText('新しい順に5件を表示しています（全9件）。')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'すべて表示（9件）' }));
    expect(nums()).toEqual(['9', '応', '応', '6', '5', '応', '応', '2', '1']);
    const list = query('.lo-events');
    const heads = list.querySelectorAll('.lo-event-head');
    expect(heads[0]?.querySelector('.lo-result strong')?.textContent).toBe('15 → 3 件');
    expect(heads[0]?.querySelector('.lo-result small')?.textContent).toBe('差 −12 件（前は掲載後24日分・後は11日分）');
    expect(list.textContent).not.toMatch(/増えた|減った|効果|効いた/u);
    fireEvent.click(screen.getByRole('button', { name: '最新5件だけにする' }));
    expect(nums()).toHaveLength(5);
  });

  it('必要なときだけ使うもの（案・手直し・ほかの分析・電話ヒアリング）は畳んだ「作業」に入り、開くとすぐ使える', () => {
    render(<JobCopyScreen />);
    const work = query('details.lo-work');
    expect((work as HTMLDetailsElement).open).toBe(false);
    expect(work.querySelector('summary')?.textContent).toContain('作業');
    expect(within(work).getByRole('button', { name: 'ほかの分析を開く' })).toBeTruthy();
    expect(work.querySelector('details.ph-section summary')?.textContent).toContain('電話ヒアリング');
    // いつも見るもの（ライフライン・応募理由・出来事）は作業の外にある。
    for (const name of ['求人のライフライン', '応募理由（ヒアリング内容）', '出来事（新しい順）']) expect(work.contains(screen.getByRole('heading', { name }))).toBe(false);
    fireEvent.click(query('details.lo-work > summary'));
    expect((query('details.lo-work') as HTMLDetailsElement).open).toBe(true);
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

  it('応募者・応募理由の前後は横から出て、閉じられる。「ほかの分析」は入口の1つから開き、タブは7つ', () => {
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
    expect(within(screen.getByRole('tablist', { name: '求人管理の機能' })).getAllByRole('tab').map(tab => tab.textContent)).toEqual(['タイムライン', '本文・画像', '変更差分', '求人比較', '応募者構成', '応募理由', '要因・仮説']);
  });
});
