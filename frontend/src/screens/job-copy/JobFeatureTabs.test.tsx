// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { JobFeaturePanel, JobFeatureTabs } from './JobFeatureTabs';

afterEach(cleanup);
describe('functional job tabs', () => {
  it('opens the last visited child rather than resetting the selected function', () => {
    const change = vi.fn();
    render(<JobFeatureTabs value="body" onChange={change} remembered={{ market: 'market-table' }} prefix="test">content</JobFeatureTabs>);
    const tabs = screen.getByRole('tablist', { name: '求人管理の機能' });
    fireEvent.click(within(tabs).getByRole('tab', { name: '市場分析' }));
    expect(change).toHaveBeenCalledWith('market-table');
    expect(screen.queryByRole('tab', { name: '市場グラフ' })).toBeNull();
    // 求人内容はサブタブが 1 つだけなので、サブタブの段を出さない
    expect(screen.queryByRole('tab', { name: '本文・画像' })).toBeNull();
    expect(screen.getByRole('tab', { name: '求人内容' }).getAttribute('tabindex')).toBe('0');
    expect(screen.getByRole('tab', { name: '求人内容' }).getAttribute('aria-selected')).toBe('true');
  });
  it('supports keyboard selection and valid primary/secondary panel relationships', () => {
    const change = vi.fn();
    render(<JobFeatureTabs value="applications" onChange={change} remembered={{}} prefix="keys"><JobFeaturePanel feature="applications" active prefix="keys">application content</JobFeaturePanel></JobFeatureTabs>);
    const primary = within(screen.getByRole('tablist', { name: '求人管理の機能' }));
    fireEvent.keyDown(primary.getByRole('tab', { name: '応募分析' }), { key: 'ArrowRight' });
    expect(change).toHaveBeenLastCalledWith('market');
    fireEvent.keyDown(primary.getByRole('tab', { name: '応募分析' }), { key: 'Home' });
    expect(change).toHaveBeenLastCalledWith('timeline');
    fireEvent.keyDown(primary.getByRole('tab', { name: '応募分析' }), { key: 'End' });
    // データ取込はタブの列から外した（外部文面は「データ取込」から開く）。最後は比較・報告。
    expect(change).toHaveBeenLastCalledWith('diff');
    expect(within(screen.getByRole('tablist', { name: '求人管理の機能' })).getAllByRole('tab').map(tab => tab.textContent)).toEqual(['タイムライン', '求人内容', '応募分析', '市場分析', '比較・報告']);
    const leaf = screen.getByRole('tab', { name: '応募推移' });
    expect(document.getElementById(leaf.getAttribute('aria-controls') ?? '')).toBe(screen.getByRole('tabpanel', { name: '応募推移' }));
    fireEvent.keyDown(leaf, { key: 'ArrowLeft' });
    expect(change).toHaveBeenLastCalledWith('performance');
  });
  it('names single-feature panels by the group tab and shows the hidden external-text group with a way back', () => {
    const leave = vi.fn();
    const { unmount } = render(<JobFeatureTabs value="timeline" onChange={vi.fn()} remembered={{}} prefix="single"><JobFeaturePanel feature="timeline" active prefix="single">timeline content</JobFeaturePanel></JobFeatureTabs>);
    const panel = document.getElementById('single-panel-timeline');
    expect(panel?.getAttribute('aria-labelledby')).toBe('single-group-timeline');
    expect(screen.getAllByRole('tabpanel', { name: 'タイムライン' })).toContain(panel);
    expect(screen.queryByRole('tablist', { name: 'タイムラインの表示' })).toBeNull();
    unmount();
    render(<JobFeatureTabs value="receive" onChange={vi.fn()} remembered={{}} prefix="hidden" onLeaveHidden={leave}><JobFeaturePanel feature="receive" active prefix="hidden">receive content</JobFeaturePanel></JobFeatureTabs>);
    const tabs = within(screen.getByRole('tablist', { name: '求人管理の機能' })).getAllByRole('tab');
    expect(tabs.map(tab => tab.getAttribute('aria-selected'))).toEqual(['false', 'false', 'false', 'false', 'false']);
    expect(tabs.map(tab => tab.getAttribute('tabindex'))).toEqual(['0', '-1', '-1', '-1', '-1']);
    const receive = document.getElementById('hidden-panel-receive');
    expect(receive?.textContent).toBe('receive content');
    expect(screen.getAllByRole('tabpanel', { name: '外部文面を確認' })).toContain(receive);
    fireEvent.click(screen.getByRole('button', { name: 'タイムラインに戻る' }));
    expect(leave).toHaveBeenCalledTimes(1);
  });
});
