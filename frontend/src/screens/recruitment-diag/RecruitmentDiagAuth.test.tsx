// @vitest-environment happy-dom
// Session expiry while the panels load goes to the login page (same redirectToLogin as the shell),
// not to a "取得失敗: login required (HTTP 401)" panel.
import { cleanup, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as fx from './fixtures';
import { loadAllPanels, type PanelName } from './loader';
import type { PanelView } from './panels';
import { EMPTY_FORM, type DiagnosisForm } from './query';
import { makeFiltersProvider } from './testFilters';
import { urlOf } from './testUtils';
import { RecruitmentDiag } from './RecruitmentDiagScreen';

vi.mock('../../components/EChart', () => ({
  EChart: (props: { testId: string }) => <div data-testid={props.testId} />,
}));
vi.mock('../../shell/navigation', () => ({ redirectToLogin: vi.fn() }));

import { redirectToLogin } from '../../shell/navigation';

const form: DiagnosisForm = { ...EMPTY_FORM, jobType: '小売業', prefecture: '東京都', prefcode: 13 };

const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

beforeEach(() => {
  vi.mocked(redirectToLogin).mockClear();
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

async function load(only401: PanelName[] | 'all'): Promise<Partial<Record<PanelName, PanelView>>> {
  vi.stubGlobal(
    'fetch',
    vi.fn<typeof fetch>((input) => {
      const name = new URL(urlOf(input), 'http://localhost').pathname.split('/').pop() as PanelName;
      if (only401 === 'all' || only401.includes(name)) return Promise.resolve(json({ error: 'unauthorized' }, 401));
      return Promise.resolve(json(name === 'difficulty' ? fx.difficulty : fx.talentPool));
    }),
  );
  const views: Partial<Record<PanelName, PanelView>> = {};
  await loadAllPanels(form, new AbortController().signal, (n, v) => {
    views[n] = v;
  });
  return views;
}

describe('401 while loading the panels', () => {
  it('redirects to the login page once and shows no "取得失敗" panel for it', async () => {
    const views = await load('all');
    expect(redirectToLogin).toHaveBeenCalledTimes(1);
    // only the static "under development" panel (Panel 3) has a view
    expect(Object.keys(views)).toEqual(['inflow']);
  });

  it('one panel answering 401 is enough to redirect', async () => {
    const views = await load(['competitors']);
    expect(redirectToLogin).toHaveBeenCalledTimes(1);
    expect(views.competitors).toBeUndefined();
    expect(views.difficulty?.status).toBe('done');
  });

  it('a 500 is still that panel\'s error and does not redirect', async () => {
    vi.stubGlobal('fetch', vi.fn<typeof fetch>(() => Promise.resolve(json({}, 500))));
    const views: Partial<Record<PanelName, PanelView>> = {};
    await loadAllPanels(form, new AbortController().signal, (n, v) => {
      views[n] = v;
    });
    expect(redirectToLogin).not.toHaveBeenCalled();
    expect(views.difficulty?.status).toBe('error');
    render(<>{views.difficulty?.body}</>);
  });

  it('the geo lists (prefectures) answering 401 also go to the login page', async () => {
    vi.stubGlobal('fetch', vi.fn<typeof fetch>(() => Promise.resolve(json({ error: 'x' }, 401))));
    const { Provider } = makeFiltersProvider();
    render(
      <Provider>
        <RecruitmentDiag />
      </Provider>,
    );
    await waitFor(() => {
      expect(redirectToLogin).toHaveBeenCalled();
    });
  });
});
