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
    expect(screen.getByRole('tab', { name: '本文・画像' }).getAttribute('tabindex')).toBe('0');
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
    expect(change).toHaveBeenLastCalledWith('receive');
    const leaf = screen.getByRole('tab', { name: '応募推移' });
    expect(document.getElementById(leaf.getAttribute('aria-controls') ?? '')).toBe(screen.getByRole('tabpanel', { name: '応募推移' }));
    fireEvent.keyDown(leaf, { key: 'ArrowLeft' });
    expect(change).toHaveBeenLastCalledWith('performance');
  });
});
