import { act, fireEvent, screen, within } from '@testing-library/react';
import { jobFeatureGroups } from './JobFeatureTabs';
import { updateJobCopyQuery } from './navigation';

/**
 * 「この求人でやること」の札「くわしい分析を開く」から横の画面を開き、feature のタブ（例: 本文と画像）に切り替える。
 * タブに並べない機能（タイムラインなど）は、画面と同じく URL の feature で開く。
 */
export function openOtherAnalysis(feature: string): HTMLElement {
  const hidden = jobFeatureGroups.find(group => group.hidden && group.features.some(item => item.label === feature))?.features.find(item => item.label === feature);
  if (hidden) {
    act(() => { updateJobCopyQuery({ feature: hidden.id, analysis: '1', drawer: null, drawerEvent: null }); });
    return screen.getByRole('dialog', { name: 'くわしい分析' });
  }
  if (!screen.queryByRole('dialog', { name: 'くわしい分析' })) {
    const section = screen.getByRole('region', { name: 'この求人でやること' });
    fireEvent.click(within(section).getByRole('button', { name: /^くわしい分析を開く/ }));
  }
  const dialog = screen.getByRole('dialog', { name: 'くわしい分析' });
  const tab = within(dialog).getByRole('tab', { name: feature });
  if (tab.getAttribute('aria-selected') !== 'true') fireEvent.click(tab);
  return dialog;
}
