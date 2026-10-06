// @vitest-environment happy-dom
// 旧 static/js/competitor-tabs.js と同じ操作: クリック・←→(端で回り込む)・Home・End。
import { useState } from 'react';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { Tabs } from './Tabs';
import { ReportView } from './ReportView';
import { makeReport } from './fixtures';

afterEach(cleanup);

const IDS = ['excel', 'google', 'indeed', 'population', 'consultation'] as const;

function Harness({ start = 'excel' }: { start?: (typeof IDS)[number] }) {
  const [sel, setSel] = useState<string>(start);
  return (
    <Tabs
      label="競合調査の表示切り替え"
      selected={sel}
      onSelect={setSel}
      items={IDS.map((id) => ({
        id,
        label: `タブ-${id}`,
        content: <p>内容-{id}</p>,
      }))}
    />
  );
}

const tab = (id: string): HTMLElement => screen.getByRole('tab', { name: `タブ-${id}` });
const selectedIds = (): string[] =>
  IDS.filter((id) => tab(id).getAttribute('aria-selected') === 'true');

describe('Tabs の構造', () => {
  it('tablist に label があり、tab は 5 つ、id と aria-controls が旧と同じ規則', () => {
    render(<Harness />);
    expect(screen.getByRole('tablist').getAttribute('aria-label')).toBe('競合調査の表示切り替え');
    expect(screen.getAllByRole('tab')).toHaveLength(5);
    expect(tab('excel').id).toBe('tab-excel');
    expect(tab('excel').getAttribute('aria-controls')).toBe('panel-excel');
    expect(tab('population').getAttribute('aria-controls')).toBe('panel-population');
  });

  it('選択は 1 つだけ。tabIndex は選択中 0、他は -1', () => {
    render(<Harness />);
    expect(selectedIds()).toEqual(['excel']);
    expect(tab('excel').tabIndex).toBe(0);
    expect(tab('google').tabIndex).toBe(-1);
  });

  it('パネルは 5 つとも DOM にあり、非選択は hidden', () => {
    const { container } = render(<Harness />);
    const panels = IDS.map((id) => container.querySelector<HTMLElement>(`#panel-${id}`));
    expect(panels.every((p) => p !== null)).toBe(true);
    expect(panels.map((p) => p?.hidden)).toEqual([false, true, true, true, true]);
    expect(panels[0]?.getAttribute('role')).toBe('tabpanel');
    expect(panels[0]?.getAttribute('aria-labelledby')).toBe('tab-excel');
  });
});

describe('Tabs のキー操作', () => {
  it('クリックで切り替わる', () => {
    const { container } = render(<Harness />);
    fireEvent.click(tab('indeed'));
    expect(selectedIds()).toEqual(['indeed']);
    expect(container.querySelector<HTMLElement>('#panel-indeed')?.hidden).toBe(false);
    expect(container.querySelector<HTMLElement>('#panel-excel')?.hidden).toBe(true);
  });

  it('→ は次へ、最後の次は先頭へ回り込み、フォーカスも動く', () => {
    render(<Harness />);
    fireEvent.keyDown(tab('excel'), { key: 'ArrowRight' });
    expect(selectedIds()).toEqual(['google']);
    expect(document.activeElement).toBe(tab('google'));
    fireEvent.keyDown(tab('google'), { key: 'ArrowRight' });
    fireEvent.keyDown(tab('indeed'), { key: 'ArrowRight' });
    expect(selectedIds()).toEqual(['population']);
    fireEvent.keyDown(tab('population'), { key: 'ArrowRight' });
    expect(selectedIds()).toEqual(['consultation']);
    fireEvent.keyDown(tab('consultation'), { key: 'ArrowRight' });
    expect(selectedIds()).toEqual(['excel']);
  });

  it('← は前へ、先頭の前は最後へ回り込む', () => {
    render(<Harness />);
    fireEvent.keyDown(tab('excel'), { key: 'ArrowLeft' });
    expect(selectedIds()).toEqual(['consultation']);
    expect(document.activeElement).toBe(tab('consultation'));
    fireEvent.keyDown(tab('consultation'), { key: 'ArrowLeft' });
    expect(selectedIds()).toEqual(['population']);
  });

  it('Home は先頭、End は最後', () => {
    render(<Harness start="indeed" />);
    fireEvent.keyDown(tab('indeed'), { key: 'End' });
    expect(selectedIds()).toEqual(['consultation']);
    fireEvent.keyDown(tab('consultation'), { key: 'Home' });
    expect(selectedIds()).toEqual(['excel']);
    expect(document.activeElement).toBe(tab('excel'));
  });

  it('矢印の既定動作 (スクロール) は止める。無関係なキーは何もしない', () => {
    render(<Harness />);
    const prevented = !fireEvent.keyDown(tab('excel'), { key: 'ArrowRight' });
    expect(prevented).toBe(true);
    const other = fireEvent.keyDown(tab('google'), { key: 'a' });
    expect(other).toBe(true);
    expect(selectedIds()).toEqual(['google']);
  });
});

describe('ReportView のタブ', () => {
  it('5 つの見出しは旧画面と同じ文言で、給与・待遇が最初に選択される', () => {
    const { container } = render(
      <ReportView report={makeReport()} tab="excel" onTabChange={() => undefined} />,
    );
    expect(screen.getAllByRole('tab').map((t) => t.textContent)).toEqual([
      '給与・待遇',
      'Google検索需要',
      'Indeed採用レポート',
      '人口・地域データ',
      '採用のヒント',
    ]);
    expect(screen.getByRole('tab', { name: '給与・待遇' }).getAttribute('aria-selected')).toBe(
      'true',
    );
    const hidden = ['google', 'indeed', 'population', 'consultation'].map(
      (id) => container.querySelector<HTMLElement>(`#panel-${id}`)?.hidden,
    );
    expect(hidden).toEqual([true, true, true, true]);
  });

  it('選択すると onTabChange に ID が渡る', () => {
    const seen: string[] = [];
    render(
      <ReportView
        report={makeReport()}
        tab="excel"
        onTabChange={(t) => {
          seen.push(t);
        }}
      />,
    );
    fireEvent.click(screen.getByRole('tab', { name: 'Indeed採用レポート' }));
    expect(seen).toEqual(['indeed']);
  });
});
