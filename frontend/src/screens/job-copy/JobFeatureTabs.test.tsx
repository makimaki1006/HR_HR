// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { JobFeaturePanel, JobFeatureTabs, jobFeatureMenu } from './JobFeatureTabs';

afterEach(cleanup);
describe('functional job tabs', () => {
  it('shows six flat tabs and opens the remembered function of a group', () => {
    const change = vi.fn();
    render(<JobFeatureTabs value="body" onChange={change} remembered={{ content: 'body' }} prefix="test">content</JobFeatureTabs>);
    const tabs = screen.getByRole('tablist', { name: '求人管理の機能' });
    expect(within(tabs).getAllByRole('tab').map(tab => tab.textContent)).toEqual(['タイムライン', '本文・画像', '変更差分', '求人比較', '応募者構成', '応募理由']);
    expect(jobFeatureMenu.map(feature => feature.id)).toEqual(['timeline', 'body', 'diff', 'ab', 'applicants', 'reasons']);
    // サブタブの段は無い
    expect(screen.queryByRole('tablist', { name: '本文・画像の表示' })).toBeNull();
    expect(screen.getByRole('tab', { name: '本文・画像' }).getAttribute('tabindex')).toBe('0');
    expect(screen.getByRole('tab', { name: '本文・画像' }).getAttribute('aria-selected')).toBe('true');
    fireEvent.click(within(tabs).getByRole('tab', { name: '応募理由' }));
    expect(change).toHaveBeenCalledWith('reasons');
  });
  it('supports keyboard selection and valid tab/panel relationships', () => {
    const change = vi.fn();
    render(<JobFeatureTabs value="diff" onChange={change} remembered={{}} prefix="keys"><JobFeaturePanel feature="diff" active prefix="keys">diff content</JobFeaturePanel></JobFeatureTabs>);
    const primary = within(screen.getByRole('tablist', { name: '求人管理の機能' }));
    fireEvent.keyDown(primary.getByRole('tab', { name: '変更差分' }), { key: 'ArrowRight' });
    expect(change).toHaveBeenLastCalledWith('ab');
    fireEvent.keyDown(primary.getByRole('tab', { name: '変更差分' }), { key: 'ArrowLeft' });
    expect(change).toHaveBeenLastCalledWith('body');
    fireEvent.keyDown(primary.getByRole('tab', { name: '変更差分' }), { key: 'Home' });
    expect(change).toHaveBeenLastCalledWith('timeline');
    fireEvent.keyDown(primary.getByRole('tab', { name: '変更差分' }), { key: 'End' });
    expect(change).toHaveBeenLastCalledWith('reasons');
    const leaf = screen.getByRole('tab', { name: '変更差分' });
    expect(document.getElementById(leaf.getAttribute('aria-controls') ?? '')).toBe(document.getElementById('keys-group-panel'));
    const panel = document.getElementById('keys-panel-diff');
    expect(panel?.getAttribute('aria-labelledby')).toBe('keys-group-diff');
    expect(screen.getAllByRole('tabpanel', { name: '変更差分' })).toContain(panel);
  });
  it('opens a screen taken out of the tabs by its URL, with its own heading and a way back', () => {
    const leave = vi.fn();
    render(<JobFeatureTabs value="market-table" onChange={vi.fn()} remembered={{}} prefix="hidden" onLeaveHidden={leave}><JobFeaturePanel feature="market-table" active prefix="hidden">table content</JobFeaturePanel></JobFeatureTabs>);
    const tabs = within(screen.getByRole('tablist', { name: '求人管理の機能' })).getAllByRole('tab');
    expect(tabs.map(tab => tab.getAttribute('aria-selected'))).toEqual(['false', 'false', 'false', 'false', 'false', 'false']);
    expect(tabs.map(tab => tab.getAttribute('tabindex'))).toEqual(['0', '-1', '-1', '-1', '-1', '-1']);
    expect(screen.getByRole('heading', { name: '市場データ' })).toBeTruthy();
    const panel = document.getElementById('hidden-panel-market-table');
    expect(panel?.textContent).toBe('table content');
    expect(screen.getAllByRole('tabpanel', { name: '市場データ' })).toContain(panel);
    fireEvent.click(screen.getByRole('button', { name: 'タイムラインに戻る' }));
    expect(leave).toHaveBeenCalledTimes(1);
  });
});
